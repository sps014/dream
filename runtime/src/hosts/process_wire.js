export function encodeProcessArgs(args) {
  const encoded = args.map(argument => new TextEncoder().encode(argument));
  const size = encoded.reduce((total, bytes) => total + 4 + bytes.length, 4);
  const wire = new Uint8Array(size);
  const view = new DataView(wire.buffer);
  view.setUint32(0, encoded.length, true);
  let position = 4;
  for (const bytes of encoded) {
    view.setUint32(position, bytes.length, true);
    position += 4;
    wire.set(bytes, position);
    position += bytes.length;
  }
  return wire;
}

export function splitProcessArgs(input) {
  const bytes = Uint8Array.from(input);
  const view = new DataView(bytes.buffer);
  const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
  let position = 0;
  const length = () => {
    if (bytes.length - position < 4) throw new Error("invalid process argument encoding");
    const value = view.getUint32(position, true);
    position += 4;
    return value;
  };
  const count = length();
  if (count > Math.floor((bytes.length - position) / 4)) throw new Error("invalid process argument count");
  const args = [];
  for (let i = 0; i < count; i++) {
    const size = length();
    if (size > bytes.length - position) throw new Error("truncated process argument");
    const argument = decoder.decode(bytes.subarray(position, position + size));
    if (argument.includes("\0")) throw new Error("NUL in process argument");
    args.push(argument);
    position += size;
  }
  if (position !== bytes.length) throw new Error("trailing process argument data");
  return args;
}
