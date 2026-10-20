import test from "node:test";
import assert from "node:assert/strict";
import { readGuestArray, readGuestString, writeGuestArray, writeGuestString } from "../runtime/src/typed.js";
import { GUEST_ABI_VERSION } from "../runtime/src/guest_contract.js";
import { pumpRunLoop } from "../runtime/src/async.js";
import { DreamInstance } from "../runtime/src/instance.js";

test("scheduler traps reject pending run observers", async () => {
  const failure = new WebAssembly.RuntimeError("guest trap");
  const inst = { i32: () => 0, exports: { __dream_run_loop() { throw failure; } } };
  pumpRunLoop(inst);
  await Promise.resolve();
  await assert.rejects(DreamInstance.prototype.__awaitFuture.call(inst, 16), error => error === failure);
});

function fixture() {
  const memory = new WebAssembly.Memory({ initial: 1, maximum: 4 });
  const spans = new Map();
  let next = 64;
  const alloc = (length, width) => {
    const reference = next;
    const pointer = next + 16;
    next = Math.ceil((pointer + length * width) / 16) * 16;
    spans.set(reference, [pointer, length]);
    return reference;
  };
  return { memory, exports: {
    __dream_guest_version: () => GUEST_ABI_VERSION,
    __dream_guest_length: reference => spans.get(reference)?.[1] ?? 0,
    __dream_guest_data: reference => spans.get(reference)?.[0] ?? 0,
    __dream_guest_array: alloc,
    __dream_guest_string: length => alloc(length, 2),
  } };
}

test("typed spans preserve UTF-16 units, integer widths, and array snapshots", () => {
  const inst = fixture();
  const text = "日本語\0😀\ud800";
  assert.equal(readGuestString(inst, writeGuestString(inst, text)), text);
  const numbers = [-(1n << 63n), (1n << 63n) - 1n];
  const reference = writeGuestArray(inst, numbers, BigInt64Array);
  const copy = readGuestArray(inst, reference, BigInt64Array, true);
  inst.memory.grow(1);
  assert.deepEqual([...copy], numbers);
  assert.deepEqual([...readGuestArray(inst, reference, BigInt64Array, false)], numbers);
});

test("typed adapters reject ABI mismatches before allocating and validate span bounds", () => {
  const inst = fixture();
  let allocated = false;
  inst.exports.__dream_guest_array = () => { allocated = true; return 0; };
  inst.exports.__dream_guest_version = () => GUEST_ABI_VERSION + 1;
  assert.throws(() => writeGuestArray(inst, [1], Uint8Array), /version mismatch/);
  assert.equal(allocated, false);
  inst.exports.__dream_guest_version = () => GUEST_ABI_VERSION;
  inst.exports.__dream_guest_data = () => 65535;
  inst.exports.__dream_guest_length = () => 2;
  assert.throws(() => readGuestArray(inst, 1, Uint32Array, false), /out of bounds/);
});
