// Node runner for the fetch interop sample.
//
//   cargo run -- sample/interop/fetch.dream
//   node sample/interop/fetch.mjs sample/interop/fetch.wasm
//
// Node 18+ has a global `fetch`, so Fetch.text / Fetch.get work against real URLs out of the box.
// Here we stub `globalThis.fetch` so the sample is self-contained and deterministic offline.

import { run } from "../../runtime/dream.js";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const wasmPath = process.argv[2] || here + "fetch.wasm";

globalThis.fetch = (url, init = {}) => {
  const method = (init.method || "GET").toUpperCase();
  if (method === "POST") {
    // Echo the posted body back with a 201.
    return Promise.resolve({
      status: 201,
      ok: true,
      text: () => Promise.resolve(`created: ${init.body || ""}`),
    });
  }
  if (String(url).endsWith("/user")) {
    return Promise.resolve({
      status: 200,
      ok: true,
      text: () => Promise.resolve(JSON.stringify({ name: "Ada", age: 36 })),
    });
  }
  return Promise.resolve({
    status: 200,
    ok: true,
    text: () => Promise.resolve("hello from fetch"),
  });
};

await run(wasmPath);
