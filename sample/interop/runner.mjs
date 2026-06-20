// Node runner for the MinLang interop sample.
//
//   node sample/interop/runner.mjs            # defaults to interop.wasm beside this file
//   node sample/interop/runner.mjs other.wasm
//
// `alert` and `console.log` auto-bind to Node globals, so we only supply `square`.

import { run } from "../../runtime/minlang.js";
import { fileURLToPath } from "node:url";

// The browser has a global `alert`; Node does not, so provide one for auto-bind to find.
globalThis.alert ??= (msg) => console.log(`[alert] ${msg}`);

const here = fileURLToPath(new URL(".", import.meta.url));
const wasmPath = process.argv[2] || here + "interop.wasm";

await run(wasmPath, {
  imports: { square: (n) => n * n },
});
