/** WASM custom-section name written by `embed_abi_in_wasm` (`src/driver/abi.rs`). */
export const ABI_CUSTOM_SECTION = "dream-abi";

/**
 * ABI JSON baked into the module. `run("./mod.wasm")` should not need a sibling fetch.
 */
export function readEmbeddedAbi(wasmModule) {
  if (typeof WebAssembly.Module.customSections !== "function") return null;
  const secs = WebAssembly.Module.customSections(wasmModule, ABI_CUSTOM_SECTION);
  if (!secs.length) return null;
  return JSON.parse(new TextDecoder("utf-8").decode(secs[0]));
}

/**
 * Swap a Dream artifact extension (`.wasm` / `.abi.json` / `.wgsl`), keeping `?query` / `#hash`.
 * Fallback when an older `.wasm` has no embedded ABI section.
 */
export function replaceArtifactExt(source, ext) {
  if (typeof source !== "string") return undefined;
  const q = source.search(/[?#]/);
  const path = q < 0 ? source : source.slice(0, q);
  const extra = q < 0 ? "" : source.slice(q);
  const stem = path.endsWith(".abi.json")
    ? path.slice(0, -9)
    : path.endsWith(".wasm")
      ? path.slice(0, -5)
      : path.endsWith(".wgsl")
        ? path.slice(0, -5)
        : null;
  if (stem == null) return undefined;
  return stem + ext + extra;
}
