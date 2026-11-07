import assert from "node:assert/strict";
import test from "node:test";
import { TAGS, LEN_PREFIX_SIZE } from "../runtime/src/abi.js";
import { DreamInstance } from "../runtime/src/instance.js";
import { decodeJsSlots, JS_SLOT } from "../runtime/src/marshal.js";

function instance() {
  const memory = new WebAssembly.Memory({ initial: 1 });
  let next = 128;
  return new DreamInstance({ exports: { memory, malloc(size) {
    const pointer = next;
    next += size;
    return pointer;
  } } });
}

test("wasm target-sized arrays preserve width and signedness", () => {
  const guest = instance();
  for (const [type, values] of [["isize", [-2147483648, -1, 42]], ["usize", [4294967295, 2147483648, 42]]]) {
    const pointer = guest.writeArray(values, type);
    assert.deepEqual(guest.readArray(pointer, type), values);
  }
});

test("dynamic slots decode target-sized scalars and array elements", () => {
  const guest = instance();
  const values = [4294967295, 2147483648, 42];
  const array = guest.writeArray(values, "usize");
  guest.view.setInt32(16, JS_SLOT.ISIZE, true);
  guest.view.setInt32(24, -1, true);
  guest.view.setInt32(32, JS_SLOT.USIZE, true);
  guest.view.setUint32(40, 4294967295, true);
  guest.view.setInt32(48, JS_SLOT.ARRAY, true);
  guest.view.setInt32(52, JS_SLOT.USIZE, true);
  guest.view.setInt32(56, array, true);
  assert.deepEqual(decodeJsSlots(guest, 16, 3), [-1, 4294967295, values]);
});


test("character arrays preserve Unicode without widening byte arrays", () => {
  const guest = instance();
  const chars = [0x41, 0x3a9, 0x754c, 0x1f600];
  const pointer = guest.writeArray(chars, "char");
  assert.deepEqual(guest.readArray(pointer, "char"), chars);
  assert.equal(guest.view.getInt32(pointer + LEN_PREFIX_SIZE + 3 * 4, true), 0x1f600);
  const bytes = guest.writeArray([0, 128, 255], "byte");
  assert.deepEqual(
    [...guest.bytes.slice(bytes + LEN_PREFIX_SIZE, bytes + LEN_PREFIX_SIZE + 3)],
    [0, 128, 255],
  );
});

test("worker Future recognition uses shape bits and ignores shared/weak flags", () => {
  const guest = instance();
  const pointer = 96;
  for (const tag of [TAGS.FUTURE, TAGS.FUTURE_KIND | 42, TAGS.FUTURE_KIND | TAGS.SHARED | 42, TAGS.FUTURE_KIND | TAGS.REGION | 42, (TAGS.FUTURE_KIND | 42) | 0x80000000]) {
    guest.view.setInt32(pointer - TAGS.FROM_DATA, tag, true);
    assert.equal(guest.__isFutureFrame(pointer), true);
  }
  for (const tag of [TAGS.ARRAY, TAGS.ARRAY_KIND | 42, TAGS.STRING, 42]) {
    guest.view.setInt32(pointer - TAGS.FROM_DATA, tag, true);
    assert.equal(guest.__isFutureFrame(pointer), false);
  }
  assert.equal(guest.__isFutureFrame(0), false);
});
