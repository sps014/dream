import test from "node:test";
import assert from "node:assert/strict";
import { makeDataHost } from "../runtime/src/hosts/data.js";
const host = makeDataHost();
const request = value => JSON.parse(host.dataCodec(JSON.stringify(value)));
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
