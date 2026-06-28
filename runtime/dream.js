// Dream JS interop runtime.
//
// Loads a Dream-compiled `.wasm` module, wires the default `env` builtins, and lets you bind
// JavaScript implementations to `extern fun` declarations with automatic value marshaling for
// strings, arrays, `List<T>`, and structs. Works as an ES module in both the browser and Node.
//
// Usage (browser):
//   import { load } from "./dream.js";
//   const mod = await load("interop.wasm", {
//     abi: "interop.abi.json",            // optional; enables auto-marshaling of imports
//     imports: { alert: (msg) => window.alert(msg) },
//   });
//   mod.run();                            // calls exported `main`
//
// Usage (Node >= 18):
//   import { load } from "./dream.js";
//   const mod = await load("interop.wasm", { imports: { alert: console.log } });
//   mod.run();

// Runtime type tags stored in each heap block header (see object.rs).
export const TAGS = {
  INT: 1,
  FLOAT: 2,
  DOUBLE: 3,
  BOOL: 4,
  STRING: 5,
  ARRAY: 6,
  STRUCT_BASE: 7,
};

// Byte size of the universal heap-block header: [size:i32][tag:i32][ref_count:i32].
// Allocated pointers point at `data` (block_start + HEAP_HEADER_SIZE).
export const HEAP_HEADER_SIZE = 12;

/** Byte size of a single element of the given Dream type (see utils.rs `element_size_of`). */
function elementSize(typeName) {
  if (typeName === "bool") return 1;
  if (typeName === "double") return 8;
  return 4; // int, float, and every reference type (pointer)
}

/** Strips a trailing `?` (nullable) and `[]` (array) suffix from a type name. */
function stripSuffix(typeName) {
  let t = typeName;
  if (t.endsWith("?")) t = t.slice(0, -1);
  return t;
}

const isPrimitive = (t) => t === "int" || t === "float" || t === "double" || t === "bool";

/**
 * A loaded Dream module instance. Exposes the raw WASM exports plus helpers that understand
 * Dream's heap layout so you can read/write strings, arrays, lists, and structs.
 */
export class DreamInstance {
  constructor(instance) {
    this.instance = instance;
    this.exports = instance.exports;
    this.memory = instance.exports.memory;
  }

  /** A fresh DataView over current memory (memory may grow, so do not cache the buffer). */
  get view() {
    return new DataView(this.memory.buffer);
  }

  /** A fresh Uint8Array over current memory. */
  get bytes() {
    return new Uint8Array(this.memory.buffer);
  }

  // --- raw scalar reads -----------------------------------------------------
  i32(ptr) {
    return this.view.getInt32(ptr, true);
  }
  f32(ptr) {
    return this.view.getFloat32(ptr, true);
  }
  f64(ptr) {
    return this.view.getFloat64(ptr, true);
  }

  /** Reads a null-terminated UTF-8 string at `ptr` (a Dream string data pointer). */
  readString(ptr) {
    if (!ptr) return "";
    const bytes = this.bytes;
    let end = ptr;
    while (end < bytes.length && bytes[end] !== 0) end++;
    return new TextDecoder("utf-8").decode(bytes.subarray(ptr, end));
  }

  /**
   * Allocates a Dream string block for `str` and returns its data pointer, so JS-implemented
   * extern functions can return strings back into Dream. Requires the module to export `malloc`.
   */
  writeString(str) {
    if (typeof this.exports.malloc !== "function") {
      throw new Error("module does not export `malloc`; cannot allocate a string");
    }
    const encoded = new TextEncoder().encode(str);
    const ptr = this.exports.malloc(encoded.length + 1, TAGS.STRING);
    const bytes = this.bytes;
    bytes.set(encoded, ptr);
    bytes[ptr + encoded.length] = 0; // null terminator
    return ptr;
  }

  /** Reads a single element of `elemType` at byte address `addr`. */
  _readElement(addr, elemType) {
    const t = stripSuffix(elemType);
    switch (t) {
      case "int":
        return this.i32(addr);
      case "bool":
        return this.bytes[addr] !== 0;
      case "float":
        return this.f32(addr);
      case "double":
        return this.f64(addr);
      case "string":
        return this.readString(this.i32(addr));
      default:
        if (t.endsWith("[]")) return this.readArray(this.i32(addr), t.slice(0, -2));
        return this.i32(addr); // struct/object/list: opaque pointer
    }
  }

  /**
   * Reads a Dream array at data pointer `ptr` into a JS array. Layout: [count:i32] followed by
   * `count` elements of `elemType`.
   */
  readArray(ptr, elemType = "int") {
    if (!ptr) return [];
    const count = this.i32(ptr);
    const size = elementSize(elemType);
    const out = new Array(count);
    for (let i = 0; i < count; i++) {
      out[i] = this._readElement(ptr + 4 + i * size, elemType);
    }
    return out;
  }

  /**
   * Reads a `List<T>` at data pointer `ptr` into a JS array. A List is a struct `{ items: T[];
   * count: int }`, so `items` is at offset 0 and the logical length at offset 4.
   */
  readList(ptr, elemType = "int") {
    if (!ptr) return [];
    const itemsPtr = this.i32(ptr);
    const count = this.i32(ptr + 4);
    const size = elementSize(elemType);
    const out = new Array(count);
    for (let i = 0; i < count; i++) {
      out[i] = this._readElement(itemsPtr + 4 + i * size, elemType);
    }
    return out;
  }

  /**
   * Reads a struct at data pointer `ptr` using a schema describing its fields in declaration
   * order. Schema entries are `{ name, type }`; offsets are derived from element sizes.
   */
  readStruct(ptr, schema) {
    const out = {};
    let offset = 0;
    for (const field of schema) {
      out[field.name] = this._readElement(ptr + offset, field.type);
      offset += elementSize(field.type);
    }
    return out;
  }

  /**
   * Reserved for JS -> Dream callbacks (phase 2). Passing a Dream function to JS so JS can
   * invoke it requires a WASM funcref table + `call_indirect` and an exported indirect function
   * table, which the compiler does not yet emit. Calling this today throws.
   */
  callback(_handle) {
    throw new Error(
      "Dream -> JS callbacks are not yet supported (phase 2: requires an exported funcref table)"
    );
  }

  /** Calls the exported `main`, if present. Returns its result (if any). */
  run() {
    if (typeof this.exports.main === "function") {
      return this.exports.main();
    }
    throw new Error("module has no exported `main`");
  }
}

/** Marshals raw WASM argument values into JS values per the parameter type names. */
function marshalArgs(inst, params, rawArgs) {
  if (!params) return rawArgs;
  return rawArgs.map((arg, i) => {
    const t = params[i] ? stripSuffix(params[i]) : "int";
    if (t === "string") return inst.readString(arg);
    if (t.endsWith("[]")) return inst.readArray(arg, t.slice(0, -2));
    if (t === "bool") return arg !== 0;
    return arg; // numeric primitive or opaque pointer
  });
}

/** Marshals a JS return value back into the raw WASM value for the declared result type. */
function marshalResult(inst, result, ret) {
  if (result === "string") return inst.writeString(ret == null ? "" : String(ret));
  if (result === "bool") return ret ? 1 : 0;
  if (result === "void" || result == null) return ret == null ? 0 : ret;
  return ret;
}

/** Wraps a user-provided import implementation so its args/return are marshaled per the ABI. */
function wrapImport(getInstance, fn, signature) {
  const params = signature ? signature.params : null;
  const result = signature ? signature.result : null;

  return (...rawArgs) => {
    const inst = getInstance();
    const args = marshalArgs(inst, params, rawArgs);
    const ret = fn(...args);
    return marshalResult(inst, result, ret);
  };
}

// Future heap kinds/sizes (mirrors src/codegen/wasm/async_support.rs).
const FUTURE_KIND_HOST = 1;
const FUTURE_SLOTS_SIZE = 56; // F_SLOTS: a host future has no saved-locals region.

/**
 * Wraps an `extern async` import. The JS implementation returns a Promise; the wrapper
 * synchronously allocates a host `Future` and hands its pointer back to Dream, then resolves it
 * (and re-pumps the scheduler) once the Promise settles. This is the only place the JS `.then`
 * bridge lives - Dream source never sees a Promise.
 */
function wrapAsyncImport(getInstance, fn, signature) {
  const params = signature ? signature.params : null;
  const result = signature ? signature.result : null;

  return (...rawArgs) => {
    const inst = getInstance();
    const exports = inst.exports;
    if (typeof exports.__dream_new_future !== "function") {
      throw new Error("module does not export the async runtime; cannot bridge an extern async import");
    }
    const args = marshalArgs(inst, params, rawArgs);
    const future = exports.__dream_new_future(FUTURE_SLOTS_SIZE, -1, FUTURE_KIND_HOST);
    Promise.resolve(fn(...args)).then((value) => {
      exports.__dream_resolve(future, marshalResult(inst, result, value));
      exports.__dream_run_loop();
    });
    return future;
  };
}

/**
 * Resolves an extern import against the JS global scope so common APIs need no boilerplate.
 * The `env` module maps to a bare global (e.g. `alert`); any other module maps to a property of
 * that global object (e.g. `console.log`, `Math.max`). Returns the function bound to its owner,
 * or `undefined` if there is no matching global function.
 */
function resolveGlobal(module, field) {
  if (module === "env") {
    const g = globalThis[field];
    return typeof g === "function" ? g.bind(globalThis) : undefined;
  }
  const owner = globalThis[module];
  const fn = owner && owner[field];
  return typeof fn === "function" ? fn.bind(owner) : undefined;
}

/** Default `env` builtins every Dream module imports (mirrors src/.../wasm_runner.rs). */
function defaultEnv(getInstance, options) {
  const writeOut = options.stdout || ((s) => (typeof process !== "undefined" ? process.stdout.write(s) : console.log(s)));
  const writeLine = options.stdout
    ? (s) => options.stdout(s + "\n")
    : (s) => console.log(s);

  return {
    print_string: (ptr) => writeOut(getInstance().readString(ptr)),
    println: (ptr) => writeLine(getInstance().readString(ptr)),
    print_int: (v) => writeOut(String(v)),
    print_float: (v) => writeOut(String(v)),
    print_double: (v) => writeOut(String(v)),
    print_char: (v) => writeOut(String.fromCharCode(v)),
    sin: Math.sin,
    cos: Math.cos,
    abs: Math.abs,
    sqrt: Math.sqrt,
  };
}

/** True when running under Node (vs. a browser), used to pick the byte-loading strategy. */
const isNode = typeof process !== "undefined" && !!(process.versions && process.versions.node);

/** Fetches `.wasm`/`.abi.json` bytes from a URL or local file path, in browser or Node. */
async function fetchBytes(source) {
  if (source instanceof ArrayBuffer) return new Uint8Array(source);
  if (source instanceof Uint8Array) return source;
  // In a browser, always go through `fetch` - a bare relative path like "app.wasm" is a valid
  // URL there and must not fall through to the Node-only `fs` branch.
  if (!isNode && typeof fetch === "function") {
    const res = await fetch(source);
    if (!res.ok) throw new Error(`failed to fetch ${source}: ${res.status}`);
    return new Uint8Array(await res.arrayBuffer());
  }
  // Node fallback.
  const { readFile } = await import("node:fs/promises");
  return new Uint8Array(await readFile(source));
}

async function loadAbi(abi) {
  if (!abi) return null;
  if (typeof abi === "object" && abi.externs) return abi; // already parsed
  const bytes = await fetchBytes(abi);
  return JSON.parse(new TextDecoder("utf-8").decode(bytes));
}

/**
 * Loads and instantiates a Dream module.
 *
 * @param {string|ArrayBuffer|Uint8Array} source - URL/path to `.wasm`, or raw bytes.
 * @param {object} [options]
 * @param {object} [options.imports] - JS implementations keyed by extern function name.
 * @param {string|object} [options.abi] - URL/path to (or parsed) `.abi.json` for auto-marshaling.
 * @param {function} [options.stdout] - Custom output sink for print builtins.
 * @returns {Promise<DreamInstance>}
 */
export async function load(source, options = {}) {
  const wasmBytes = await fetchBytes(source);
  const abi = await loadAbi(options.abi);

  // Late-bound instance reference so import wrappers can marshal against live memory.
  let instance = null;
  const getInstance = () => {
    if (!instance) throw new Error("instance not ready");
    return instance;
  };

  // Build the import object: default env builtins first, then user-provided externs.
  const importObject = { env: defaultEnv(getInstance, options) };

  const userImports = options.imports || {};
  const sigByName = new Map();
  if (abi) for (const e of abi.externs) sigByName.set(e.name, e);

  const wrapFor = (fn, sig) =>
    sig && sig.async ? wrapAsyncImport(getInstance, fn, sig) : wrapImport(getInstance, fn, sig);

  // 1. User-supplied implementations win, keyed by extern (Dream function) name.
  for (const name of Object.keys(userImports)) {
    const sig = sigByName.get(name);
    const module = sig ? sig.module : "env";
    const field = sig ? sig.field : name;
    (importObject[module] ||= {})[field] = wrapFor(userImports[name], sig);
  }

  // 2. Auto-bind any remaining externs to matching JS globals so built-in APIs need no glue
  //    (e.g. `alert`, `@js("console","log")`, `@js("Math","max")`). Unresolved imports get a
  //    thrower stub so instantiation still succeeds and the error only surfaces if called.
  if (abi) {
    for (const e of abi.externs) {
      const bucket = (importObject[e.module] ||= {});
      if (bucket[e.field]) continue; // already provided by the user
      const resolved = resolveGlobal(e.module, e.field);
      bucket[e.field] = resolved
        ? wrapFor(resolved, e)
        : () => {
            throw new Error(`no JS implementation for extern '${e.name}' (${e.module}.${e.field})`);
          };
    }
  }

  const { instance: wasmInstance } = await WebAssembly.instantiate(wasmBytes, importObject);
  instance = new DreamInstance(wasmInstance);
  return instance;
}

/**
 * load a module and immediately invoke its `main`. The `.abi.json` path is
 * derived from the `.wasm` URL unless `options.abi` is given, so a whole page can be just:
 *
 *   import { run } from "./dream.js";
 *   await run("app.wasm", { imports: { ... } });
 *
 * @returns {Promise<DreamInstance>} the loaded instance (after `main` has run).
 */
export async function run(source, options = {}) {
  const abi =
    options.abi ?? (typeof source === "string" ? source.replace(/\.wasm$/, ".abi.json") : undefined);
  const mod = await load(source, { ...options, abi });
  mod.run();
  return mod;
}

export default { load, run, DreamInstance, TAGS, HEAP_HEADER_SIZE };
