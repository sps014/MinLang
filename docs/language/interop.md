# JS Interop

Dream compiles to WebAssembly, so it runs anywhere WASM does — including the browser and Node. The `extern` keyword lets a Dream program call out to JavaScript with almost no boilerplate, PyScript-style.

## Declaring an extern function

An `extern fun` has a signature but no body. It is lowered to a WebAssembly *import* instead of a defined function:

```kotlin
extern fun alert(msg: string): void;

fun main(): void {
    alert("Hello from Dream!");
}
```

By default the import comes from the `env` module under the function's own name. You call it like any other function.

## Remapping the import name

Use the `@js(module, name)` attribute to control which import module and field the extern binds to:

```kotlin
// binds to importObject["dom"]["setText"]
@js("dom", "setText")
extern fun set_text(value: string): void;

// only the module given -> field defaults to the function name
@js("console")
extern fun log(msg: string): void;
```

!!! note "Restrictions"
    Extern functions cannot have a body, cannot be generic, and cannot be combined with `pub`.

## Running it from JavaScript

Compiling a `.dream` file automatically produces three artifacts next to it:

- `*.wat` — the human-readable WebAssembly text.
- `*.wasm` — the binary module browsers and Node load.
- `*.abi.json` — an auto-generated description of the extern imports and exports. You never write or edit this; the runtime reads it to marshal values for you.

The `runtime/dream.js` ES module loads the `.wasm`, wires the built-in `print`/math functions, and runs `main`. The `run` helper derives the sibling `.abi.json` automatically, so a whole page can be one call:

```javascript
import { run } from "./runtime/dream.js";

await run("hello.wasm");   // loads hello.abi.json, binds externs, calls main
```

### Auto-binding to JS globals

Most externs need no glue at all. For every extern you do not supply, the runtime resolves it against the JavaScript global scope:

- The default `env` module maps to a bare global — `extern fun alert(...)` binds to `alert`.
- `@js("module", "name")` maps to a property of that global — `@js("console", "log")` binds to `console.log`, `@js("Math", "max")` to `Math.max`.

So built-in browser/Node APIs work with zero boilerplate. You only pass `imports` for your own custom logic:

```javascript
await run("hello.wasm", {
  imports: {
    square: (n) => n * n,   // keyed by the Dream function name
  },
});
```

If an extern matches no global and you do not provide it, the runtime installs a stub that throws only if it is actually called — so the module still instantiates.

When you need full control, use `load(source, options)` instead of `run`; it returns the instance without calling `main`.

## Value marshaling

With the ABI loaded, arguments and return values are converted between Dream's heap layout and JavaScript:

| Dream type | JavaScript value (as argument) | As return value |
|--------------|-------------------------------|-----------------|
| `int`, `float`, `double` | `number` | `number` |
| `bool` | `boolean` | `boolean` |
| `string` | `string` (decoded UTF-8) | return a `string` |
| `T[]` | `Array` of marshaled elements | (pointer) |
| `object`, structs, `List<T>` | opaque pointer (`number`) | (pointer) |

For reference types you can read the underlying data with the instance helpers:

```javascript
mod.readString(ptr);          // null-terminated UTF-8 string
mod.readArray(ptr, "int");    // -> number[]
mod.readList(ptr, "string");  // List<string> -> string[]
mod.readStruct(ptr, [         // struct by field schema (declaration order)
  { name: "x", type: "int" },
  { name: "y", type: "int" },
]);
```

To hand a string back to Dream from a JS implementation, the runtime calls the exported `malloc` for you (or you can call `mod.writeString(str)` directly).

## References and callbacks (planned)

Reference types cross the boundary as opaque `i32` pointers and are read with the helpers above; there is no general "JavaScript object into Dream" path yet, since Dream's only dynamic type is `object` (a boxed primitive or struct).

Passing a function as an argument is also not supported yet:

- A Dream function handed to JavaScript so JS can call it back requires an exported WebAssembly `funcref` table and `call_indirect`. The runtime reserves `mod.callback(handle)` for this and throws until the feature lands.
- A JavaScript function passed into a Dream `extern` parameter cannot be expressed today, because Dream has no function/closure type.
