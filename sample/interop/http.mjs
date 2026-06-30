// Node runner for the HttpClient interop sample.
//
//   cargo run -- sample/interop/http.dream
//   node sample/interop/http.mjs sample/interop/http.wasm
//
// Node 18+ has a global `fetch`, so HttpClient works against real URLs out of the box. Here we stub
// `globalThis.fetch` (returning real `Response` objects) so the sample is self-contained and
// deterministic offline. The native build (`cargo run -- ...`) uses a real blocking HTTP request.

import { run } from "../../runtime/dream.js";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const wasmPath = process.argv[2] || here + "http.wasm";

globalThis.fetch = async (url, init = {}) => {
  const method = (init.method || "GET").toUpperCase();
  const u = String(url);
  if (method === "POST" || method === "PUT") {
    const len = init.body ? init.body.length : 0;
    return new Response(`created (${len} bytes)`, {
      status: 201,
      headers: { "content-type": "text/plain" },
    });
  }
  if (u.endsWith("/user")) {
    return new Response(JSON.stringify({ name: "Ada", age: 36 }), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  }
  if (u.endsWith("/blob")) {
    return new Response(new Uint8Array([1, 2, 3, 4]), {
      status: 200,
      headers: { "content-type": "application/octet-stream" },
    });
  }
  return new Response("hello from http", {
    status: 200,
    headers: { "content-type": "text/plain" },
  });
};

await run(wasmPath);
