// Node runner for the async Dream interop sample.
//
//   cargo run -- sample/interop/async_fetch.dream
//   node sample/interop/async_runner.mjs sample/interop/async_fetch.wasm
//
// `getUser` is an `extern async` import: returning a Promise is enough. dream.js allocates a host
// Future, hands its pointer back to Dream synchronously, and resolves it (re-pumping the scheduler)
// once the Promise settles. In a browser this would be `fetch(...).then((r) => r.text())`.

import { run } from "../../runtime/dream.js";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const wasmPath = process.argv[2] || here + "async_fetch.wasm";

// Simulate a network call: resolve after a short delay.
const fakeUser = (id) =>
  new Promise((resolve) => setTimeout(() => resolve(`user#${id}`), 20));

await run(wasmPath, {
  imports: {
    getUser: fakeUser,
  },
});
