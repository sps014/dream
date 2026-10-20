import { loadBinaryModule } from "../binary.js";
import { bindDataCodecs } from "../capabilities.js";

const codecModules = new Map();
const codecInstances = new Map();

export async function loadDataCodecs(field, source) {
  if (!codecModules.has(field)) codecModules.set(field, await loadBinaryModule(source));
}

function requestCodec(field, request) {
  const input = new TextEncoder().encode(request);
  if (input.length > 16 * 1024 * 1024) return JSON.stringify({ error: "codec request exceeds 16 MiB" });
  try {
    if (!codecInstances.has(field)) {
      const module = codecModules.get(field);
      if (!module) throw new Error("codec family was not loaded: " + field);
      const instance = new WebAssembly.Instance(module, {});
      codecInstances.set(field, bindDataCodecs(instance.exports));
    }
    return new TextDecoder("utf-8", { fatal: true }).decode(codecInstances.get(field)(input));
  } catch (_) {
    // A trap can abandon Rust allocations; a fresh instance restores its allocator.
    codecInstances.delete(field);
    return JSON.stringify({ error: "data codec adapter failed" });
  }
}

export function makeDataHost() {
  return {
    dataCsv: request => requestCodec("dataCsv", request),
    dataCompression: request => requestCodec("dataCompression", request),
  };
}
