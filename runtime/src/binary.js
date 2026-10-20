import { isNode } from "./platform.js";

export async function loadBinaryModule(source) {
  if (source instanceof WebAssembly.Module) return source;
  let bytes = source;
  if (!(source instanceof Uint8Array) && !(source instanceof ArrayBuffer)) {
    if (isNode) {
      const { readFile } = await import("node:fs/promises");
      bytes = await readFile(source);
    } else {
      const response = await fetch(source);
      if (!response.ok) throw new Error(`failed to fetch capability: ${response.status}`);
      bytes = await response.arrayBuffer();
    }
  }
  return WebAssembly.compile(bytes);
}
