// Node runner for the bidirectional-callbacks sample.
//
//   cargo run -- sample/interop/callbacks.dream
//   node sample/interop/callbacks.mjs sample/interop/callbacks.wasm

import { run } from "../../runtime/dream.js";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const wasmPath = process.argv[2] || here + "callbacks.wasm";

// A JS function Dream will call back into (JS -> Dream callback), reachable via js_global.
globalThis.logger = (msg) => console.log("[logger]", msg);

await run(wasmPath, {
  imports: {
    // `cb` arrives already wrapped as a JS callable (the runtime bridges the funcref table).
    run_callback: (cb, times) => {
      for (let i = 0; i < times; i++) cb(i);
    },
  },
});
