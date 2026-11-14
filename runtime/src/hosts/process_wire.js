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
