// Node runner for the regex sample.
//
//   cargo run -- sample/interop/pattern.dream
//   node sample/interop/regex.mjs sample/interop/pattern.wasm
//
// Regex needs no custom imports: PCRE2 is linked into the module.

import { run } from "../../runtime/dream.js";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const wasmPath = process.argv[2] || here + "target/web/pattern.wasm";

await run(wasmPath);
