// Node runner for value-struct <-> JS object marshaling.
//
//   cargo run -- sample/interop/value_structs.dream
//   node sample/interop/value_structs.mjs sample/interop/value_structs.wasm

import { run } from "../../runtime/dream.js";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const wasmPath = process.argv[2] || here + "target/web/value_structs.wasm";

globalThis.receiveValueUser = (u) => {
  console.log("received:", JSON.stringify(u));
};
globalThis.makeValueUser = () => ({
  name: "Grace",
  age: 45,
  home: { x: 1, y: 2 },
});

await run(wasmPath);
