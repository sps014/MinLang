// Node runner for the regex interop sample.
//
//   cargo run -- sample/interop/regex.dream
//   node sample/interop/regex.mjs sample/interop/regex.wasm
//
// Regex needs no custom imports: the `Dream` host module in runtime/dream.js backs the
// regexTest/regexReplace/regexMatchJoined helpers with JavaScript's RegExp automatically.

import { run } from "../../runtime/dream.js";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const wasmPath = process.argv[2] || here + "regex.wasm";

await run(wasmPath);
