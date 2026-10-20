import test from "node:test";
import assert from "node:assert/strict";
import { loadDataCodecs, makeDataHost } from "../runtime/src/hosts/data.js";
import { bindDataCodecs, CAPABILITY_ABI_VERSION } from "../runtime/src/capabilities.js";
import { readFileSync } from "node:fs";
await loadDataCodecs("dataCsv", new URL("../runtime/dream-csv.wasm", import.meta.url));
await loadDataCodecs("dataCompression", new URL("../runtime/dream-compression.wasm", import.meta.url));
const host = makeDataHost();
const request = value => JSON.parse(host[value.action.startsWith("csv_") ? "dataCsv" : "dataCompression"](JSON.stringify(value)));

test("generated scalar/span adapter checks versions and survives memory growth", () => {
  const instance = new WebAssembly.Instance(new WebAssembly.Module(
    readFileSync(new URL("../runtime/dream-csv.wasm", import.meta.url)),
  ), {});
  const exports = instance.exports;
  assert.throws(() => bindDataCodecs({ ...exports, dream_capability_abi_version: () => CAPABILITY_ABI_VERSION + 1 }), /version mismatch/);
  assert.throws(() => bindDataCodecs({ ...exports, dream_data_request: undefined }), /missing capability export/);
  const call = bindDataCodecs(exports);
  const decode = bytes => JSON.parse(new TextDecoder().decode(bytes));
  assert.match(decode(call(new Uint8Array([255]))).error, /UTF-8/);
  const previous = exports.memory.buffer;
  const fields = ["日本語".repeat(100000), "a,b", 'say "hi"'];
  const encoded = decode(call(new TextEncoder().encode(JSON.stringify({ action: "csv_write", delimiter: 44, fields }))));
  assert.equal(encoded.error, undefined);
  assert.notEqual(exports.memory.buffer, previous);
  const parsed = decode(call(new TextEncoder().encode(JSON.stringify({ action: "csv_read", delimiter: 44, input: encoded.text }))));
  assert.deepEqual(parsed.fields, fields);
});
test("CSV codec preserves quoted fields, newlines, Unicode, and custom delimiters", () => {
  const fields = ['a;b', 'say "hi"', 'line\r\nnext', 'こんにちは', ''];
  const encoded = request({action:'csv_write',delimiter:59,fields});
  assert.equal(encoded.error, undefined);
  assert.ok(encoded.text.endsWith('\r\n'));
  assert.deepEqual(request({action:'csv_read',delimiter:59,input:encoded.text}).fields, fields);
  assert.ok(request({action:'csv_read',delimiter:10,input:'a,b'}).error);
});
test("shared WASM compression validates checksums, truncation, and expansion limits", () => {
  const create = (direction,limit) => request({action:'compress_create',format:'gzip',direction,limit}).id;
  const step = (id,input,finish=true) => request({action:'compress_step',id,input,finish,capacity:65536});
  const encoder = create('encode',1000);
  const encoded = step(encoder,Buffer.from('hello compression').toString('hex'));
  assert.equal(encoded.finished,true);
  const decoder = create('decode',1000);
  assert.equal(Buffer.from(step(decoder,encoded.output).output,'hex').toString(),'hello compression');
  const limited = create('decode',2);
  assert.ok(step(limited,encoded.output).error);
  const truncated = create('decode',1000);
  step(truncated,encoded.output.slice(0,10));
  assert.ok(step(truncated,'').error);
  for (const id of [encoder,decoder,limited,truncated]) request({action:'compress_close',id});
});
