import { GPU_BINDING_LAYOUTS, GPU_BUNDLE_ABI_VERSION } from "./gpu_layouts.js";
import { loadBinaryModule } from "../binary.js";

let gpuParser;
class GpuBundleError extends Error {}

export async function loadGpuParser(source) {
  if (gpuParser) return;
  const module = await loadBinaryModule(source);
  const instance = new WebAssembly.Instance(module, {});
  const exports = instance.exports;
  if (exports.dream_gpu_bundle_abi_version?.() !== GPU_BUNDLE_ABI_VERSION) {
    throw new GpuBundleError("GPU bundle capability ABI version mismatch");
  }
  for (const name of ["gpu_bundle_alloc", "gpu_bundle_reset", "dream_gpu_bundle_index", "dream_gpu_bundle_error"]) {
    if (typeof exports[name] !== "function") throw new GpuBundleError("missing GPU parser export: " + name);
  }
  if (!(exports.memory instanceof WebAssembly.Memory)) throw new GpuBundleError("GPU parser has no linear memory");
  gpuParser = exports;
}

function guestParser(inst) {
  const exports = inst.exports;
  if (exports.__dream_gpu_version?.() !== GPU_BUNDLE_ABI_VERSION) throw new GpuBundleError("GPU bundle capability ABI version mismatch");
  const references = [];
  return {
    memory: inst.memory,
    dream_gpu_bundle_index: exports.__dream_gpu_index,
    gpu_bundle_alloc(length) {
      const reference = exports.__dream_guest_array(length, 1);
      references.push(reference);
      return exports.__dream_guest_data(reference, 0);
    },
    gpu_bundle_reset() {
      for (const reference of references) inst.__guestFree(reference);
      references.length = 0;
    },
  };
}

export function gpuParseBundle(input, inst) {
  const parser = inst ? guestParser(inst) : gpuParser;
  if (!parser) throw new GpuBundleError("GPU parser was not loaded");
  const bytes = input instanceof Uint8Array ? input.slice() : Uint8Array.from(input || []);
  const decoder = new TextDecoder();
  const check = status => {
    if (status >= 0) return status;
    if (inst) throw new GpuBundleError("invalid shader bundle (status " + status + ")");
    const offset = parser.dream_gpu_bundle_error(status);
    const memory = new Uint8Array(parser.memory.buffer);
    const end = memory.indexOf(0, offset);
    throw new GpuBundleError(decoder.decode(memory.subarray(offset, end < 0 ? memory.length : end)));
  };
  const text = (offset, length) => decoder.decode(bytes.subarray(offset, offset + length));
  const blob = (row, at) => ({ offset: row[at], size: row[at + 1],
    sha256: bytes.subarray(row[at + 2], row[at + 2] + 32), bytes: bytes.subarray(row[at], row[at] + row[at + 1]) });
  parser.gpu_bundle_reset();
  try {
    const borrowed = inst && input instanceof Uint8Array && input.buffer === inst.memory.buffer;
    const pointer = borrowed ? input.byteOffset : parser.gpu_bundle_alloc(bytes.length);
    if (!pointer) throw new GpuBundleError("GPU parser input allocation failed");
    if (!borrowed) new Uint8Array(parser.memory.buffer, pointer, bytes.length).set(bytes);
    const count = check(parser.dream_gpu_bundle_index(pointer, bytes.length, 0, 0));
    const records = parser.gpu_bundle_alloc(count * 64);
    if (!records) throw new GpuBundleError("GPU parser record allocation failed");
    check(parser.dream_gpu_bundle_index(pointer, bytes.length, records, count));
    const words = new Uint32Array(parser.memory.buffer, records, count * 16);
    const objects = [], targets = [];
    for (let i = 0; i < count; i++) {
      const row = words.subarray(i * 16, i * 16 + 16);
      const parent = objects[row[1]];
      let object;
      switch (row[0]) {
        case 1:
          object = { name: text(row[2], row[3]), codeFormat: text(row[4], row[5]), layouts: [], entries: [] };
          targets.push(object);
          break;
        case 2: parent.layouts.push(blob(row, 2)); break;
        case 3:
          object = { name: text(row[2], row[3]), stage: text(row[14], row[15]), layout: row[5],
            code: blob(row, 6), threadGroupSize: [row[9], row[10], row[11]], params: [], capabilities: [] };
          parent.entries.push(object);
          break;
        case 4:
          parent.params.push({ name: text(row[2], row[3]), kind: row[4], flags: row[5], group: row[6], binding: row[7],
            offset: row[8], size: row[9], webgpuLayout: GPU_BINDING_LAYOUTS[row[10]]?.[1] ?? null });
          break;
        case 5: parent.capabilities.push(text(row[2], row[3])); break;
      }
      objects.push(object);
    }
    return { version: 0, targets };
  } finally { parser.gpu_bundle_reset(); }
}

function gpuHex(bytes) {
  let out = "";
  for (const b of bytes) out += b.toString(16).padStart(2, "0");
  return out;
}

/**
 * Resolves to null when every code blob of `target` matches its SHA-256, else to the reason.
 * WebGPU only exists in secure contexts, which always provide `crypto.subtle`.
 */
export async function gpuVerifyTarget(target) {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) return "SHA-256 verification needs crypto.subtle";
  const seen = new Set();
  for (const entry of target.entries) {
    const blob = entry.code;
    const key = blob.offset + ":" + blob.size + ":" + gpuHex(blob.sha256);
    if (seen.has(key)) continue;
    seen.add(key);
    const digest = new Uint8Array(await subtle.digest("SHA-256", blob.bytes));
    if (gpuHex(digest) !== gpuHex(blob.sha256)) return "the code of " + entry.name + " fails its SHA-256 check";
  }
  return null;
}
