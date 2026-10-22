// Little-endian records shared with the guest's `WireWriter` / `WireReader` and
// `dream_host_abi::wire`: fixed-width integers, and strings or byte runs as a 32-bit length
// followed by the bytes.

const wireEncoder = new TextEncoder();
const wireDecoder = new TextDecoder("utf-8", { fatal: true });

/** Format version leading every operation header; must match `wire::VERSION`. */
export const WIRE_VERSION = 1;

function wireMalformed() {
  return Object.assign(new Error("malformed host operation"), { code: "EINVAL" });
}

export class WireWriter {
  constructor() { this.parts = []; this.size = 0; }
  raw(bytes) { this.parts.push(bytes); this.size += bytes.length; return this; }
  u8(value) { return this.raw(Uint8Array.of(value & 255)); }
  i32(value) {
    const bytes = new Uint8Array(4);
    new DataView(bytes.buffer).setInt32(0, value, true);
    return this.raw(bytes);
  }
  bytes(bytes) { this.i32(bytes.length); return this.raw(bytes instanceof Uint8Array ? bytes : Uint8Array.from(bytes)); }
  text(value) { return this.bytes(wireEncoder.encode(value)); }
  finish() {
    const out = new Uint8Array(this.size);
    let at = 0;
    for (const part of this.parts) { out.set(part, at); at += part.length; }
    return out;
  }
}

export class WireReader {
  constructor(bytes) {
    this.bytes = bytes instanceof Uint8Array ? bytes : Uint8Array.from(bytes || []);
    this.view = new DataView(this.bytes.buffer, this.bytes.byteOffset, this.bytes.byteLength);
    this.at = 0;
  }
  take(count) {
    if (!(count >= 0) || this.bytes.length - this.at < count) throw wireMalformed();
    const at = this.at;
    this.at += count;
    return at;
  }
  u8() { return this.bytes[this.take(1)]; }
  i32() { return this.view.getInt32(this.take(4), true); }
  bytesField() { const length = this.i32(); const at = this.take(length); return this.bytes.subarray(at, at + length); }
  text() {
    try { return wireDecoder.decode(this.bytesField()); } catch (_) { throw wireMalformed(); }
  }
  finish() { if (this.at !== this.bytes.length) throw wireMalformed(); }
  /** Reads `[version][action][id][timeout_ms]`; the action's arguments follow. */
  header() {
    if (this.u8() !== WIRE_VERSION) throw wireMalformed();
    const action = this.u8(), id = this.i32(), timeout_ms = this.i32();
    if (timeout_ms < 0) throw wireMalformed();
    return { action, id, timeout_ms };
  }
}

/** A failed completion: status 2, then the error code and message. */
export function wireFailure(code, message) {
  return [2, ...new WireWriter().text(code).text(message).finish()];
}
