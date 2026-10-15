import { dataCodecsBase64 } from "./data_codecs.js";

let dataCodecsInstance;
function dataCodecs() {
  if (!dataCodecsInstance) {
    const bytes = Uint8Array.from(atob(dataCodecsBase64), ch => ch.charCodeAt(0));
    dataCodecsInstance = new WebAssembly.Instance(new WebAssembly.Module(bytes), {});
  }
  return dataCodecsInstance.exports;
}

export function makeDataHost() {
  return {
    dataCodec: request => {
      const input = new TextEncoder().encode(request);
      if (input.length > 16 * 1024 * 1024) return JSON.stringify({ error: "codec request exceeds 16 MiB" });
      let parser;
      let pointer;
      let output;
      let length;
      try {
        parser = dataCodecs();
        pointer = parser.data_alloc(input.length);
        new Uint8Array(parser.memory.buffer, pointer, input.length).set(input);
        const result = BigInt.asUintN(64, parser.data_parse(pointer, input.length));
        output = Number(result >> 32n);
        length = Number(result & 0xffffffffn);
        const text = new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array(parser.memory.buffer, output, length));
        parser.data_free(pointer, input.length);
        parser.data_free(output, length);
        return text;
      } catch (_) {
        // A trap can abandon Rust allocations; a fresh capability instance restores its allocator.
        dataCodecsInstance = undefined;
        return JSON.stringify({ error: "data codec adapter failed" });
      }
    },
  };
}
