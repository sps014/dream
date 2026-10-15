import { cliParserBase64 } from "./cli_parser.js";

let cliParserInstance;
function cliParser() {
  if (!cliParserInstance) {
    const bytes = Uint8Array.from(atob(cliParserBase64), ch => ch.charCodeAt(0));
    cliParserInstance = new WebAssembly.Instance(new WebAssembly.Module(bytes), {});
  }
  return cliParserInstance.exports;
}

export function makeCliHost() {
  return {
    cliParse: request => {
      const input = new TextEncoder().encode(request);
      if (input.length > 1024 * 1024) return JSON.stringify({ kind: "error", code: "EINVAL", message: "CLI request exceeds 1 MiB" });
      let parser;
      let pointer;
      let output;
      let length;
      try {
        parser = cliParser();
        pointer = parser.cli_alloc(input.length);
        new Uint8Array(parser.memory.buffer, pointer, input.length).set(input);
        const result = BigInt.asUintN(64, parser.cli_parse(pointer, input.length));
        output = Number(result >> 32n);
        length = Number(result & 0xffffffffn);
        const text = new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array(parser.memory.buffer, output, length));
        parser.cli_free(pointer, input.length);
        parser.cli_free(output, length);
        return text;
      } catch (_) {
        // A trap can abandon Rust allocations; a fresh capability instance restores its allocator.
        cliParserInstance = undefined;
        return JSON.stringify({ kind: "error", code: "EIO", message: "CLI parser adapter failed" });
      }
    },
  };
}
