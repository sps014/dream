// Reader for the generated shader bundle (DGPB v0, docs/internals/18-gpu-shader-bundle.md).
// The browser only drives the `wgsl` target; it binds from the parameter table, so the layout
// blob is never read.

const GPU_BUNDLE_STAGES = new Set(["compute", "vertex", "fragment", "raygeneration", "closesthit",
  "anyhit", "miss", "intersection", "callable"]);
const GPU_BUNDLE_MAX_COUNT = 1 << 16;

class GpuBundleError extends Error {}

class GpuBundleReader {
  constructor(bytes, end) {
    this.bytes = bytes;
    this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    this.pos = 0;
    this.end = end;
  }

  need(count) {
    if (count < 0 || this.pos + count > this.end) throw new GpuBundleError("the bundle header is truncated");
  }

  u32() {
    this.need(4);
    const value = this.view.getUint32(this.pos, true);
    this.pos += 4;
    return value;
  }

  count(what) {
    const value = this.u32();
    if (value > GPU_BUNDLE_MAX_COUNT) throw new GpuBundleError(what + " count " + value + " is too large");
    return value;
  }

  raw(length) {
    this.need(length);
    const out = this.bytes.subarray(this.pos, this.pos + length);
    this.pos += length;
    return out;
  }

  str() {
    const length = this.u32();
    try {
      return new TextDecoder("utf-8", { fatal: true }).decode(this.raw(length));
    } catch (error) {
      if (error instanceof GpuBundleError) throw error;
      throw new GpuBundleError("a bundle string is not UTF-8");
    }
  }

  blob(dataStart) {
    const offset = this.u32();
    const size = this.u32();
    const sha256 = this.raw(32);
    const start = dataStart + offset;
    if (start + size > this.bytes.length) throw new GpuBundleError("a blob lies outside the data section");
    return { offset, size, sha256, bytes: this.bytes.subarray(start, start + size) };
  }
}

function gpuParseParam(r) {
  return {
    name: r.str(),
    kind: r.u32(),
    flags: r.u32(),
    group: r.u32(),
    binding: r.u32(),
    offset: r.u32(),
    size: r.u32(),
  };
}

function gpuParseEntry(r, dataStart) {
  const name = r.str();
  const stage = r.str();
  if (!GPU_BUNDLE_STAGES.has(stage)) throw new GpuBundleError("entry " + name + " has unknown stage '" + stage + "'");
  const code = r.blob(dataStart);
  const threadGroupSize = [r.u32(), r.u32(), r.u32()];
  const params = [];
  for (let i = 0, n = r.count("parameter"); i < n; i++) params.push(gpuParseParam(r));
  const capabilities = [];
  for (let i = 0, n = r.count("capability"); i < n; i++) capabilities.push(r.str());
  return { name, stage, code, threadGroupSize, params, capabilities };
}

/**
 * Parses a whole bundle. Throws `GpuBundleError` with a readable reason for any malformed input;
 * never reads outside `bytes`.
 */
export function gpuParseBundle(bytes) {
  if (!(bytes instanceof Uint8Array)) bytes = Uint8Array.from(bytes || []);
  if (bytes.length < 16) throw new GpuBundleError("the bundle is shorter than its header");
  const head = new GpuBundleReader(bytes, 16);
  const magic = new TextDecoder().decode(head.raw(4));
  if (magic !== "DGPB") throw new GpuBundleError("the bundle does not start with DGPB");
  const version = head.u32();
  if (version !== 0) throw new GpuBundleError("bundle version " + version + " is not supported");
  const headerSize = head.u32();
  if (headerSize < 16 || headerSize > bytes.length) throw new GpuBundleError("the bundle header size is out of range");
  const targetCount = head.count("target");
  const r = new GpuBundleReader(bytes, headerSize);
  r.pos = 16;
  const targets = [];
  for (let t = 0; t < targetCount; t++) {
    const name = r.str();
    const codeFormat = r.str();
    const layout = r.blob(headerSize);
    const entries = [];
    for (let i = 0, n = r.count("entry"); i < n; i++) entries.push(gpuParseEntry(r, headerSize));
    targets.push({ name, codeFormat, layout, entries });
  }
  return { version, targets };
}

function gpuHex(bytes) {
  let out = "";
  for (const b of bytes) out += b.toString(16).padStart(2, "0");
  return out;
}

/**
 * Resolves to null when every code blob of `target` matches its SHA-256, else to the reason.
 * WebGPU only exists in secure contexts, which always provide `crypto.subtle`.
 */
export async function gpuVerifyTarget(target) {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) return "SHA-256 verification needs crypto.subtle";
  const seen = new Set();
  for (const entry of target.entries) {
    const blob = entry.code;
    if (seen.has(blob.offset)) continue;
    seen.add(blob.offset);
    const digest = new Uint8Array(await subtle.digest("SHA-256", blob.bytes));
    if (gpuHex(digest) !== gpuHex(blob.sha256)) return "the code of " + entry.name + " fails its SHA-256 check";
  }
  return null;
}

// WGSL resource declarations by "group:binding". The parameter table says what a binding holds;
// the declaration supplies what the bind group layout additionally needs (access mode, sample
// type, storage format).
export function gpuScanWgslBindings(source) {
  const out = new Map();
  const declaration = /((?:@\w+\s*\(\s*\d+\s*\)\s*)+)var\s*(<[^>]*>)?\s*[A-Za-z_]\w*\s*:\s*([^;]+);/g;
  for (const m of source.matchAll(declaration)) {
    const group = /@group\s*\(\s*(\d+)\s*\)/.exec(m[1]);
    const binding = /@binding\s*\(\s*(\d+)\s*\)/.exec(m[1]);
    if (!group || !binding) continue;
    out.set(group[1] + ":" + binding[1], { space: (m[2] || "").replace(/\s+/g, ""), type: m[3].trim() });
  }
  return out;
}
