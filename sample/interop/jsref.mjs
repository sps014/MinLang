// Node runner for the JsRef interop sample.
//
//   cargo run -- sample/interop/jsref.dream
//   node sample/interop/jsref.mjs sample/interop/jsref.wasm
//
// `js_global("appConfig")` resolves to whatever `globalThis.appConfig` is on the host. The
// runtime registers the object in its handle table and hands Dream a small i32 id; reads and
// method calls go back through that handle. No per-function glue is needed - the `Dream` host
// module (jsGlobal/jsGetProp/jsCall0/...) ships with runtime/dream.js.

import { run } from "../../runtime/dream.js";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const wasmPath = process.argv[2] || here + "jsref.wasm";

globalThis.appConfig = {
  title: "Hello, Dream",
  count: 7,
  enabled: true,
};

await run(wasmPath);

console.log("appConfig.touched =", globalThis.appConfig.touched);
