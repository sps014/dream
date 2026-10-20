import { GUEST_ABI_VERSION } from "./guest_contract.js";
// Typed imports borrow synchronous inputs. Deferred and user-supplied hosts get snapshots,
// because their callbacks may grow guest memory or outlive the original argument frame.
function guestContext(inst) {
  const exports = inst.exports;
  if (exports.__dream_guest_version?.() !== GUEST_ABI_VERSION) throw new Error("guest capability ABI version mismatch");
  return exports;
}

export function guestSpan(inst, reference, kind, width) {
  const exports = guestContext(inst);
  const length = exports.__dream_guest_length(reference) >>> 0;
  const pointer = exports.__dream_guest_data(reference, kind) >>> 0;
  const size = inst.memory.buffer.byteLength;
  if (pointer > size || length > Math.floor((size - pointer) / width)) throw new Error("guest capability span is out of bounds");
  return [pointer, length];
}

export function readGuestString(inst, reference) {
  const [pointer, length] = guestSpan(inst, reference, 1, 2);
  const units = new Uint16Array(inst.memory.buffer, pointer, length);
  let result = "";
  for (let i = 0; i < length; i += 8192) result += String.fromCharCode(...units.subarray(i, i + 8192));
  return result;
}

export function writeGuestString(inst, value) {
  const string = value == null ? "" : String(value);
  const reference = guestContext(inst).__dream_guest_string(string.length);
  const [pointer, length] = guestSpan(inst, reference, 1, 2);
  const units = new Uint16Array(inst.memory.buffer, pointer, length);
  for (let i = 0; i < length; i++) units[i] = string.charCodeAt(i);
  return reference;
}

export function readGuestArray(inst, reference, Type, copy) {
  const [pointer, length] = guestSpan(inst, reference, 0, Type.BYTES_PER_ELEMENT);
  if (pointer % Type.BYTES_PER_ELEMENT) {
    return new Type(new Uint8Array(inst.memory.buffer, pointer, length * Type.BYTES_PER_ELEMENT).slice().buffer);
  }
  const view = new Type(inst.memory.buffer, pointer, length);
  return copy ? view.slice() : view;
}

export function writeGuestArray(inst, value, Type, convert) {
  const values = Type.from(value ?? [], convert);
  const reference = guestContext(inst).__dream_guest_array(values.length, Type.BYTES_PER_ELEMENT);
  const [pointer, length] = guestSpan(inst, reference, 0, Type.BYTES_PER_ELEMENT);
  new Uint8Array(inst.memory.buffer, pointer, length * Type.BYTES_PER_ELEMENT).set(new Uint8Array(values.buffer));
  return reference;
}
