import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { makeGpuHost } from '../runtime/src/hosts/gpu.js';
import { GPU_BINDING_LAYOUTS } from '../runtime/src/hosts/gpu_layouts.js';
import { gpuParseBundle, loadGpuParser } from '../runtime/src/hosts/gpu_bundle.js';
import { makeWindowHost } from '../runtime/src/hosts/window.js';

await loadGpuParser(readFileSync(new URL("../tests/fixtures/gpu_parser.wasm", import.meta.url)));

function gpuScanWgslBindings(source) {
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

// ----- bundle builder (docs/internals/18-gpu-shader-bundle.md, DGPB v0) -----

const KIND = { Buffer: 1, Texture: 2, Sampler: 3, StorageTexture: 4, AccelerationStructure: 5, Value: 6, MlpWeights: 7 };
const OP = { BeginRenderPass: 1, BeginComputePass: 2, EndPass: 3, SetPipeline: 4, Bind: 5, Dispatch: 6, SetVertexBuffer: 7, SetIndexBuffer: 8, SetViewport: 9, SetScissor: 10, Draw: 11, DrawIndexed: 12, CopyBuffer: 13, TraceRays: 14 };
const WEBGPU = 6;

function u32(value) {
  const out = Buffer.alloc(4);
  out.writeUInt32LE(value >>> 0);
  return out;
}

function str(text) {
  const raw = Buffer.from(text, 'utf8');
  return Buffer.concat([u32(raw.length), raw]);
}

function fixtureLayout(p) {
  switch (p.kind) {
    case KIND.Buffer: return { buffer: { type: (p.flags & 1) ? 'read-only-storage' : 'storage' } };
    case KIND.Value: return { buffer: { type: 'uniform' } };
    case KIND.Texture: return { texture: { sampleType: 'float', viewDimension: '2d', multisampled: false } };
    case KIND.Sampler: return { sampler: { type: 'filtering' } };
    default: return {};
  }
}

/** Builds a bundle; `corrupt(code)` may return different bytes to store than were hashed. */
function buildBundle(targets, { corrupt = null } = {}) {
  const data = [];
  let dataLength = 0;
  const blobs = new Map();
  const blob = raw => {
    const bytes = Buffer.from(raw);
    const key = bytes.toString('base64');
    if (!blobs.has(key)) {
      const sha = createHash('sha256').update(bytes).digest();
      const stored = corrupt ? corrupt(bytes) : bytes;
      blobs.set(key, Buffer.concat([u32(dataLength), u32(stored.length), sha]));
      data.push(stored);
      dataLength += stored.length;
    }
    return blobs.get(key);
  };
  const parts = [];
  for (const target of [...targets].sort((a, b) => a.name.localeCompare(b.name))) {
    const layouts = target.layouts || [];
    parts.push(str(target.name), str(target.codeFormat || target.name), u32(layouts.length), ...layouts.map(blob));
    parts.push(u32(target.entries.length));
    for (const entry of target.entries) {
      parts.push(str(entry.name), str(entry.stage), u32(entry.layout || 0), blob(Buffer.from(entry.code, 'utf8')));
      for (const n of entry.threadGroupSize || [0, 0, 0]) parts.push(u32(n));
      parts.push(u32(entry.params.length));
      for (const p of entry.params) {
        parts.push(str(p.name), u32(p.kind), u32(p.flags || 0), u32(p.group || 0), u32(p.binding), u32(p.offset || 0), u32(p.size || 0), u32(p.layoutCode ?? (target.name === 'wgsl'
          ? (p.kind === 5 || p.kind === 7) ? 0 : GPU_BINDING_LAYOUTS.findIndex(row => row?.[0] === p.kind
            && JSON.stringify(row[1]) === JSON.stringify(p.webgpuLayout ?? fixtureLayout(p))) : 0)));
      }
      const caps = entry.capabilities || [];
      parts.push(u32(caps.length), ...caps.map(str));
    }
  }
  const body = Buffer.concat(parts);
  const header = Buffer.concat([Buffer.from('DGPB'), u32(0), u32(16 + body.length), u32(targets.length)]);
  return [...Buffer.concat([header, body, ...data])];
}

const SAXPY_WGSL = `
struct EntryPointParams_std140_0 { n_0 : i32, @align(8) alpha_0 : f32 };
@binding(0) @group(0) var<storage, read> x_0 : array<f32>;
@binding(1) @group(0) var<storage, read> y_0 : array<f32>;
@binding(2) @group(0) var<storage, read_write> out_0 : array<f32>;
@binding(3) @group(0) var<uniform> entryPointParams_0 : EntryPointParams_std140_0;
@compute @workgroup_size(64, 1, 1) fn saxpy(@builtin(global_invocation_id) id : vec3<u32>) {}
`;
const SAXPY = {
  name: 'saxpy', stage: 'compute', code: SAXPY_WGSL, threadGroupSize: [64, 1, 1],
  params: [
    { name: 'x', kind: KIND.Buffer, flags: 1, binding: 0 },
    { name: 'y', kind: KIND.Buffer, flags: 1, binding: 1 },
    { name: 'out', kind: KIND.Buffer, flags: 2, binding: 2 },
    { name: 'n', kind: KIND.Value, binding: 3, offset: 0, size: 4 },
    { name: 'alpha', kind: KIND.Value, binding: 3, offset: 8, size: 4 },
  ],
};

const QUAD_WGSL = `
struct Camera_std140_0 { tint_0 : vec4<f32> };
@binding(0) @group(1) var<uniform> entryPointParams_0 : Camera_std140_0;
@binding(0) @group(0) var albedo_0 : texture_2d<f32>;
@binding(1) @group(0) var linear_0 : sampler;
@vertex fn quad_vertex(@location(0) p : vec2<f32>) -> @builtin(position) vec4<f32> { return vec4<f32>(p, 0.0, 1.0); }
@fragment fn quad_fragment() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }
`;
const QUAD_VERTEX = { name: 'quad_vertex', stage: 'vertex', code: QUAD_WGSL, params: [{ name: 'tint', kind: KIND.Value, group: 1, binding: 0, offset: 0, size: 16 }] };
const QUAD_FRAGMENT = {
  name: 'quad_fragment', stage: 'fragment', code: QUAD_WGSL,
  params: [{ name: 'albedo', kind: KIND.Texture, flags: 1, binding: 0 }, { name: 'linear', kind: KIND.Sampler, binding: 1 }],
};

const wgsl = (entries, extra = {}) => buildBundle([{ name: 'wgsl', entries, ...extra }]);

// ----- recording fake WebGPU -----

class FakeBuffer {
  constructor(desc) {
    this.desc = desc;
    this.data = new Uint8Array(desc.size);
    this.destroyed = false;
  }
  async mapAsync(mode) { this.mapped = mode; }
  getMappedRange() { return this.data.buffer; }
  unmap() { this.mapped = 0; }
  destroy() { this.destroyed = true; }
}

class FakeTexture {
  constructor(desc) { this.desc = desc; this.destroyed = false; }
  createView() { return { view: this }; }
  destroy() { this.destroyed = true; }
}

class FakePass {
  constructor(kind, desc) { this.kind = kind; this.desc = desc; this.calls = []; }
}
for (const name of ['setPipeline', 'setBindGroup', 'dispatchWorkgroups', 'setVertexBuffer', 'setIndexBuffer', 'setViewport', 'setScissorRect', 'draw', 'drawIndexed', 'end']) {
  FakePass.prototype[name] = function (...args) { this.calls.push([name, ...args]); };
}

class FakeDevice {
  constructor(features) {
    this.features = new Set(features);
    this.calls = [];
    this.buffers = [];
    this.scopeErrors = [];
    this.destroyed = false;
    this.lost = new Promise(resolve => { this.loseWith = message => resolve({ reason: 'unknown', message }); });
    this.queue = {
      writeBuffer: (buffer, offset, data) => buffer.data.set(new Uint8Array(data.buffer, data.byteOffset, data.byteLength), offset),
      writeTexture: (...args) => this.calls.push(['writeTexture', ...args]),
      submit: commandBuffers => {
        for (const cb of commandBuffers) {
          this.calls.push(['submit', cb]);
          for (const [op, ...args] of cb.ops) {
            if (op === 'copyBufferToBuffer') {
              const [from, fromOffset, to, toOffset, size] = args;
              to.data.set(from.data.subarray(fromOffset, fromOffset + size), toOffset);
            }
          }
        }
      },
      onSubmittedWorkDone: async () => {},
    };
  }
  log(name, value) { this.calls.push([name, value]); return value; }
  createBuffer(desc) { const b = new FakeBuffer(desc); this.buffers.push(b); return b; }
  createTexture(desc) { return this.log('createTexture', new FakeTexture(desc)); }
  createSampler(desc) { return this.log('createSampler', { sampler: desc }); }
  createShaderModule(desc) { return this.log('createShaderModule', { module: desc }); }
  createBindGroupLayout(desc) { return this.log('createBindGroupLayout', { layout: desc }); }
  createPipelineLayout(desc) { return this.log('createPipelineLayout', { pipelineLayout: desc }); }
  createComputePipeline(desc) { return this.log('createComputePipeline', { compute: desc }); }
  createRenderPipeline(desc) { return this.log('createRenderPipeline', { render: desc }); }
  createBindGroup(desc) { return this.log('createBindGroup', { group: desc }); }
  createCommandEncoder() {
    const device = this;
    const ops = [];
    return {
      beginComputePass() { const p = new FakePass('compute'); ops.push(['pass', p]); device.lastPass = p; return p; },
      beginRenderPass(desc) { const p = new FakePass('render', desc); ops.push(['pass', p]); device.lastPass = p; return p; },
      copyBufferToBuffer(...args) { ops.push(['copyBufferToBuffer', ...args]); },
      copyTextureToBuffer(...args) { ops.push(['copyTextureToBuffer', ...args]); },
      finish() { return { ops }; },
    };
  }
  pushErrorScope(filter) { this.calls.push(['pushErrorScope', filter]); }
  async popErrorScope() { return this.scopeErrors.shift() || null; }
  destroy() { this.destroyed = true; }
  named(name) { return this.calls.filter(c => c[0] === name).map(c => c[1]); }
}

function installGpu({ features = [], adapter = true } = {}) {
  const state = { devices: [], requests: [] };
  const gpu = {
    getPreferredCanvasFormat: () => 'bgra8unorm',
    requestAdapter: async () => adapter ? {
      features: new Set(features),
      requestDevice: async desc => {
        state.requests.push(desc);
        const device = new FakeDevice(desc.requiredFeatures);
        state.devices.push(device);
        return device;
      },
    } : null,
  };
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { gpu, getGamepads: () => [] } });
  return state;
}

function uninstall() {
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: undefined });
  delete globalThis.document;
}

class FakeCanvas extends EventTarget {
  constructor() {
    super();
    this.tagName = 'CANVAS';
    this.style = {};
    this.width = 300;
    this.height = 150;
    this.tabIndex = 0;
    this.configured = null;
    this.frames = 0;
  }
  getBoundingClientRect() { return { left: 0, top: 0, width: 400, height: 200 }; }
  getContext(kind) {
    if (kind !== 'webgpu') return null;
    const canvas = this;
    return {
      configure(desc) { canvas.configured = desc; },
      unconfigure() { canvas.configured = null; },
      getCurrentTexture() { canvas.frames++; return new FakeTexture({ canvas: canvas.frames }); },
    };
  }
}

function installCanvas() {
  const canvas = new FakeCanvas();
  globalThis.document = { getElementById: id => (id === 'game' ? canvas : null), activeElement: null };
  globalThis.devicePixelRatio = 1;
  return canvas;
}

async function openDevice(host, features) {
  const gpu = installGpu({ features });
  const device = await host.gpuDeviceCreate(0, 1 << WEBGPU);
  assert.ok(device > 0, host.gpuLastError());
  return { gpu, device, fake: gpu.devices.at(-1) };
}

const bits = value => new DataView(new Float32Array([value]).buffer).getInt32(0, true);

// ----- tests -----

test('bundles parse per the documented layout and malformed bytes never throw past the host', async () => {
  const bytes = wgsl([{ ...SAXPY, capabilities: ['half', 'subgroup_basic'] }]);
  const parsed = gpuParseBundle(bytes);
  assert.equal(parsed.targets.length, 1);
  const [target] = parsed.targets;
  assert.equal(target.name, 'wgsl');
  assert.deepEqual(target.layouts, []);
  assert.equal(target.entries[0].layout, 0);
  assert.deepEqual(target.entries[0].capabilities, ['half', 'subgroup_basic']);
  assert.deepEqual(target.entries[0].threadGroupSize, [64, 1, 1]);
  assert.deepEqual(target.entries[0].params[4], { name: 'alpha', kind: 6, flags: 0, group: 0, binding: 3, offset: 8, size: 4, webgpuLayout: { buffer: { type: 'uniform' } } });

  const host = makeGpuHost();
  const { device } = await openDevice(host);
  try {
    for (let cut = 0; cut < bytes.length; cut++) {
      assert.equal(host.gpuLibraryLoad(device, bytes.slice(0, cut)), 0, 'truncated at ' + cut);
      assert.match(host.gpuLastError(), /^EINVALID\nmalformed shader bundle/);
    }
    const mutate = (at, value) => { const copy = bytes.slice(); copy[at] = value; return copy; };
    assert.equal(host.gpuLibraryLoad(device, mutate(0, 0x58)), 0);
    assert.match(host.gpuLastError(), /DGPB/);
    assert.equal(host.gpuLibraryLoad(device, mutate(4, 1)), 0);
    assert.match(host.gpuLastError(), /unsupported version/);
    assert.equal(host.gpuLibraryLoad(device, mutate(12, 0xff)), 0, 'huge target count');
    let seed = 7;
    for (let round = 0; round < 500; round++) {
      const copy = bytes.slice();
      for (let k = 0; k < 4; k++) {
        seed = (seed * 1103515245 + 12345) >>> 0;
        copy[seed % copy.length] = seed >>> 24;
      }
      const handle = host.gpuLibraryLoad(device, copy);
      if (handle === 0) assert.match(host.gpuLastError(), /^E[A-Z]+\n/);
    }
    assert.equal(host.gpuLibraryLoad(device, buildBundle([{ name: 'spirv', entries: [SAXPY] }])), 0);
    assert.equal(host.gpuLastError(), 'ENOTCOMPILED\n6');
  } finally { uninstall(); }
});

test('binding metadata is validated before WebGPU and no WGSL scan supplies defaults', async () => {
  const host = makeGpuHost();
  const { device, fake } = await openDevice(host);
  try {
    for (const layoutCode of [0, 1, 4, 5, 6, 10, 0xffffffff]) {
      const entry = { ...SAXPY, params: [{ ...SAXPY.params[0], layoutCode }] };
      assert.equal(host.gpuLibraryLoad(device, wgsl([entry])), 0, String(layoutCode));
      assert.match(host.gpuLastError(), /invalid WebGPU binding layout/);
    }
    assert.equal(fake.named('createShaderModule').length, 0);
    const layout = { storageTexture: { format: 'rgba32float', access: 'read-write', viewDimension: '2d' } };
    const entry = { name: 'store', stage: 'compute', code: '@compute @workgroup_size(1) fn store() {}',
      params: [{ name: 'image', kind: KIND.StorageTexture, binding: 0, webgpuLayout: layout }] };
    const library = host.gpuLibraryLoad(device, wgsl([entry]));
    assert.ok(library > 0, host.gpuLastError());
    assert.ok(host.gpuComputePipelineCreate(library, 'store') > 0, host.gpuLastError());
    assert.deepEqual(fake.named('createBindGroupLayout').at(-1).layout.entries,
      [{ binding: 0, visibility: 4, ...layout }]);
  } finally { host.gpuRelease(device); }
});

test('render stages sharing a binding must agree on the full layout', async () => {
  const host = makeGpuHost();
  const { device } = await openDevice(host);
  try {
    const vertex = { ...QUAD_VERTEX, params: [{ name: 'data', kind: KIND.Buffer, binding: 0, flags: 1 }] };
    const fragment = { ...QUAD_FRAGMENT, params: [{ name: 'data', kind: KIND.Buffer, binding: 0, flags: 2 }] };
    const library = host.gpuLibraryLoad(device, wgsl([vertex, fragment]));
    assert.ok(library > 0, host.gpuLastError());
    assert.equal(host.gpuRenderPipelineCreate(library, vertex.name, library, fragment.name,
      [0, 0, 0, 0, 0, 0, 0, 1, 0]), 0);
    assert.match(host.gpuLastError(), /different layouts/);
  } finally { host.gpuRelease(device); }
});

test('backend selection reports the GpuError codes the Dream side decodes', async () => {
  uninstall();
  const host = makeGpuHost();
  assert.equal(await host.gpuDeviceCreate(0, 0), 0);
  assert.match(host.gpuLastError(), /^ENODEVICE\nWebGPU is not available/);
  installGpu();
  try {
    assert.equal(await host.gpuDeviceCreate(3, (1 << 3) | (1 << WEBGPU)), 0);
    assert.match(host.gpuLastError(), /^EUNAVAILABLE\nGpuBackend\.Vulkan/);
    assert.equal(await host.gpuDeviceCreate(5, 1 << WEBGPU), 0);
    assert.equal(host.gpuLastError(), 'ENOTCOMPILED\n5');
    assert.equal(await host.gpuDeviceCreate(0, 1 << 3), 0);
    assert.equal(host.gpuLastError(), 'ENOTCOMPILED\n6');
    installGpu({ adapter: false });
    assert.equal(await host.gpuDeviceCreate(6, 1 << WEBGPU), 0);
    assert.match(host.gpuLastError(), /^ENODEVICE\nWebGPU found no adapter/);

    const gpu = installGpu({ features: ['subgroups', 'shader-f16', 'timestamp-query'] });
    const device = await host.gpuDeviceCreate(6, 1 << WEBGPU);
    assert.ok(device > 0);
    assert.deepEqual(gpu.requests[0].requiredFeatures, ['subgroups', 'shader-f16']);
    assert.equal(host.gpuDeviceBackend(device), WEBGPU);
    assert.equal(host.gpuDeviceFeatures(device), (1 << 3) | (1 << 6));
    assert.equal(host.gpuDeviceCheck(device), 0);
    const plain = await openDevice(host, []);
    assert.equal(host.gpuDeviceFeatures(plain.device), 0);
    assert.ok(plain.device !== device);
  } finally { uninstall(); }
});

test('buffer writes read back through mapAsync, with WebGPU alignment enforced', async () => {
  const host = makeGpuHost();
  const { device, fake } = await openDevice(host);
  try {
    const buffer = host.gpuBufferCreate(device, 6, 1);
    assert.ok(buffer > 0);
    assert.equal(fake.buffers[0].desc.size, 8, 'sizes round up to whole words');
    assert.equal(fake.buffers[0].desc.usage, 128 | 4 | 8, 'storage + copy src/dst');
    assert.equal(host.gpuBufferWrite(buffer, 0, [1, 2, 3, 4, 5, 6]), 0, 'a write that ends the buffer may pad');
    assert.deepEqual([...await host.gpuBufferRead(buffer, 0, 6)], [1, 2, 3, 4, 5, 6]);
    assert.deepEqual([...await host.gpuBufferRead(buffer, 1, 3)], [2, 3, 4]);
    const staging = fake.buffers.at(-1);
    assert.equal(staging.desc.usage, 1 | 8);
    assert.equal(staging.destroyed, true);
    assert.equal(host.gpuBufferWrite(buffer, 2, [9, 9]), 1);
    assert.match(host.gpuLastError(), /^EINVALID\n.*aligned/);
    assert.equal(host.gpuBufferWrite(buffer, 0, [9, 9]), 1, 'an unaligned length mid-buffer would clobber data');
    assert.equal(host.gpuBufferWrite(buffer, 4, [1, 2, 3]), 1);
    assert.match(host.gpuLastError(), /outside the buffer/);
    assert.deepEqual(await host.gpuBufferRead(buffer, 4, 8), []);
    assert.match(host.gpuLastError(), /^EINVALID\n/);
    assert.equal(host.gpuBufferCreate(device, -1, 1), 0);
    assert.equal(host.gpuBufferCreate(9999, 4, 1), 0);
    assert.match(host.gpuLastError(), /^EINVALID\n/);
  } finally { uninstall(); }
});

test('compute dispatch binds the parameter table and packs values at reflected offsets', async () => {
  const host = makeGpuHost();
  const { device, fake } = await openDevice(host);
  try {
    const library = host.gpuLibraryLoad(device, wgsl([SAXPY]));
    assert.ok(library > 0, host.gpuLastError());
    const pipeline = host.gpuComputePipelineCreate(library, 'saxpy');
    assert.ok(pipeline > 0, host.gpuLastError());
    const [layout] = fake.named('createBindGroupLayout');
    assert.deepEqual(layout.layout.entries, [
      { binding: 0, visibility: 4, buffer: { type: 'read-only-storage' } },
      { binding: 1, visibility: 4, buffer: { type: 'read-only-storage' } },
      { binding: 2, visibility: 4, buffer: { type: 'storage' } },
      { binding: 3, visibility: 4, buffer: { type: 'uniform' } },
    ]);
    const [created] = fake.named('createComputePipeline');
    assert.equal(created.compute.compute.entryPoint, 'saxpy');
    assert.equal(created.compute.compute.module.module.code, SAXPY_WGSL);

    const [x, y, out] = [1, 2, 3].map(() => host.gpuBufferCreate(device, 16, 1));
    const data = [...new Uint8Array(new Int32Array([4]).buffer), ...new Uint8Array(new Float32Array([2]).buffer)];
    const commands = [
      OP.BeginComputePass, OP.SetPipeline, pipeline,
      OP.Bind, 0, 0, KIND.Buffer, x, 0,
      OP.Bind, 0, 1, KIND.Buffer, y, 0,
      OP.Bind, 0, 2, KIND.Buffer, out, 0,
      OP.Bind, 0, 3, KIND.Value, 0, 4,
      OP.Bind, 0, 4, KIND.Value, 4, 4,
      OP.Dispatch, 1, 1, 1,
      OP.EndPass,
    ];
    assert.equal(await host.gpuSubmit(device, commands, data), 0, host.gpuLastError());
    const pass = fake.lastPass;
    assert.deepEqual(pass.calls.map(c => c[0]), ['setPipeline', 'setBindGroup', 'dispatchWorkgroups', 'end']);
    assert.deepEqual(pass.calls[2], ['dispatchWorkgroups', 1, 1, 1]);
    const group = pass.calls[1][2].group;
    assert.equal(pass.calls[1][1], 0);
    assert.equal(group.layout, layout);
    assert.deepEqual(group.entries.slice(0, 3).map(e => e.resource.buffer), fake.buffers.slice(0, 3));
    const uniform = group.entries[3].resource.buffer;
    assert.equal(uniform.desc.size, 16);
    assert.equal(uniform.desc.usage, 64 | 8);
    const view = new DataView(uniform.data.buffer);
    assert.equal(view.getInt32(0, true), 4);
    assert.equal(view.getFloat32(8, true), 2);
    assert.equal(uniform.destroyed, true, 'per-submit uniform blocks are freed after the GPU finishes');
    assert.deepEqual(fake.named('pushErrorScope').slice(-2), ['out-of-memory', 'validation']);

    const run = async (words, bytes = data) => [await host.gpuSubmit(device, words, bytes), host.gpuLastError()];
    const unbound = commands.filter((_, i) => i < 21 || i >= 27);
    assert.match((await run(unbound))[1], /^EVALIDATION\nshader parameter n was not bound/);
    assert.match((await run(commands, data.slice(0, 6)))[1], /^EVALIDATION\nthe value of alpha lies outside/);
    const wide = commands.slice();
    wide[32] = 8;
    assert.match((await run(wide, [...data, 0, 0, 0, 0]))[1], /^EVALIDATION\nthe value of alpha has 8 bytes/);
    assert.match((await run(commands.slice(0, 10)))[1], /^EVALIDATION\nthe GPU command stream is truncated/);
    assert.match((await run([OP.BeginComputePass, OP.TraceRays, 1, 1, 1, OP.EndPass]))[1], /^EUNSUPPORTED\n0$/);
    assert.match((await run([99]))[1], /^EVALIDATION\nunknown GPU command 99/);
    assert.match((await run([OP.Dispatch, 1, 1, 1]))[1], /outside a compute pass/);

    const other = await openDevice(host);
    const foreign = host.gpuBufferCreate(other.device, 16, 1);
    const crossed = commands.slice();
    crossed[7] = foreign;
    assert.equal(await host.gpuSubmit(device, crossed, data), 1);
    assert.equal(host.gpuLastError(), 'EWRONGDEVICE\nGPU handle ' + foreign);

    fake.scopeErrors.push({ message: 'binding 2 is too small' });
    assert.deepEqual(await run(commands), [1, 'EVALIDATION\nbinding 2 is too small']);
    fake.onuncapturederror({ error: { message: 'shader module failed to compile' } });
    assert.deepEqual(await run(commands), [1, 'EVALIDATION\nshader module failed to compile']);
    assert.equal(await host.gpuSubmit(device, commands, data), 0, 'the reported error is cleared');
  } finally { uninstall(); }
});

test('a bundle whose code fails SHA-256 never reaches the GPU queue', async () => {
  const host = makeGpuHost();
  const { device, fake } = await openDevice(host);
  try {
    const bundle = buildBundle([{ name: 'wgsl', entries: [SAXPY] }], {
      corrupt: bytes => { const copy = Buffer.from(bytes); copy[copy.length - 2] ^= 1; return copy; },
    });
    const library = host.gpuLibraryLoad(device, bundle);
    assert.ok(library > 0);
    const pipeline = host.gpuComputePipelineCreate(library, 'saxpy');
    const submits = fake.named('submit').length;
    assert.equal(await host.gpuSubmit(device, [OP.BeginComputePass, OP.SetPipeline, pipeline, OP.EndPass], []), 1);
    assert.match(host.gpuLastError(), /^EINVALID\nthe code of saxpy fails its SHA-256 check/);
    assert.equal(fake.named('submit').length, submits);
    assert.equal(host.gpuComputePipelineCreate(library, 'saxpy'), 0);
    assert.match(host.gpuLastError(), /SHA-256/);
  } finally { uninstall(); }
});

test('every shared code reference must carry the verified digest', async () => {
  const host = makeGpuHost();
  const { device, fake } = await openDevice(host);
  try {
    const bytes = Buffer.from(wgsl([SAXPY, { ...SAXPY, name: 'alias' }]));
    const digest = createHash('sha256').update(SAXPY_WGSL).digest();
    const first = bytes.indexOf(digest);
    const second = bytes.indexOf(digest, first + digest.length);
    assert.ok(first >= 0 && second > first);
    bytes[second] ^= 1;
    const library = host.gpuLibraryLoad(device, [...bytes]);
    assert.ok(library > 0, host.gpuLastError());
    const pipeline = host.gpuComputePipelineCreate(library, 'alias');
    assert.equal(await host.gpuSubmit(device, [OP.BeginComputePass, OP.SetPipeline, pipeline, OP.EndPass], []), 1);
    assert.match(host.gpuLastError(), /the code of alias fails its SHA-256 check/);
    assert.equal(fake.named('submit').length, 0);
  } finally { uninstall(); }
});

test('render pipelines, canvas surfaces and draws translate to WebGPU', async () => {
  const canvas = installCanvas();
  const windows = makeWindowHost();
  const host = makeGpuHost();
  const { device, fake } = await openDevice(host);
  try {
    const window = windows.windowCanvas('game', 0);
    assert.ok(window > 0);
    const library = host.gpuLibraryLoad(device, wgsl([QUAD_VERTEX, QUAD_FRAGMENT]));
    const desc = [2, 0, 2, 1, 11, 1, 1, 1, 1, 16, 0, 2, 0, 1, 0, 1, 1, 8];
    const pipeline = host.gpuRenderPipelineCreate(library, 'quad_vertex', library, 'quad_fragment', desc);
    assert.ok(pipeline > 0, host.gpuLastError());
    assert.equal(fake.named('createShaderModule').length, 1, 'entries sharing a blob share one module');
    const [{ render }] = fake.named('createRenderPipeline');
    assert.equal(render.vertex.entryPoint, 'quad_vertex');
    assert.equal(render.fragment.entryPoint, 'quad_fragment');
    assert.deepEqual(render.vertex.buffers, [{
      arrayStride: 16, stepMode: 'vertex',
      attributes: [{ shaderLocation: 0, format: 'float32x2', offset: 0 }, { shaderLocation: 1, format: 'float32x2', offset: 8 }],
    }]);
    assert.deepEqual(render.fragment.targets[0].format, 'bgra8unorm');
    assert.equal(render.fragment.targets[0].blend.color.srcFactor, 'src-alpha');
    assert.deepEqual(render.primitive, { topology: 'triangle-list', cullMode: 'back' });
    assert.deepEqual(render.depthStencil, { format: 'depth32float', depthCompare: 'less', depthWriteEnabled: true });
    const layouts = fake.named('createBindGroupLayout');
    assert.deepEqual(layouts.map(l => l.layout.entries), [
      [{ binding: 0, visibility: 2, texture: { sampleType: 'float', viewDimension: '2d', multisampled: false } },
        { binding: 1, visibility: 2, sampler: { type: 'filtering' } }],
      [{ binding: 0, visibility: 1, buffer: { type: 'uniform' } }],
    ]);
    assert.equal(host.gpuRenderPipelineCreate(library, 'quad_fragment', library, 'quad_fragment', desc), 0);
    assert.match(host.gpuLastError(), /^EVALIDATION\nquad_fragment is a fragment shader/);

    const surface = host.gpuSurfaceCreate(device, window, true);
    assert.ok(surface > 0, host.gpuLastError());
    assert.equal(canvas.configured.format, 'bgra8unorm');
    assert.deepEqual(host.gpuSurfaceInfo(surface), [2, 400, 200]);
    assert.equal(host.gpuSurfaceResize(surface, 640, 480), 0);
    assert.deepEqual([canvas.width, canvas.height], [640, 480]);
    assert.deepEqual(host.gpuSurfaceInfo(surface), [2, 640, 480]);
    assert.equal(host.gpuSurfaceResize(surface, 0, 480), 1);

    const frame = await host.gpuSurfaceAcquire(surface);
    assert.ok(frame > 0);
    const depth = host.gpuTextureCreate(device, [640, 480, 1, 1, 10, 0, 1]);
    assert.equal(fake.named('createTexture').at(-1).desc.usage, 4 | 16, 'depth textures are attachments');
    const albedo = host.gpuTextureCreate(device, [2, 2, 1, 1, 0, 0, 1]);
    assert.equal(host.gpuTextureWrite(albedo, 0, 0, new Array(16).fill(255)), 0);
    assert.equal(host.gpuTextureWrite(albedo, 0, 0, [1]), 1);
    const sampler = host.gpuSamplerCreate(device, [1, 1, 1, 1, 1, 1, 0, 4]);
    assert.deepEqual(fake.named('createSampler').at(-1).sampler, {
      minFilter: 'linear', magFilter: 'linear', mipmapFilter: 'linear',
      addressModeU: 'repeat', addressModeV: 'repeat', addressModeW: 'repeat', maxAnisotropy: 4,
    });
    const vertices = host.gpuBufferCreate(device, 64, 2);
    const indices = host.gpuBufferCreate(device, 24, 4);
    const tint = [...new Uint8Array(new Float32Array([1, 0.5, 0.25, 1]).buffer)];
    const commands = [
      OP.BeginRenderPass, frame, 1, bits(0.1), bits(0.2), bits(0.3), bits(1), depth, 1, bits(1),
      OP.SetPipeline, pipeline,
      OP.SetVertexBuffer, 0, vertices, 0,
      OP.SetIndexBuffer, indices, 1, 0,
      OP.SetViewport, bits(0), bits(0), bits(640), bits(480), bits(0), bits(1),
      OP.SetScissor, 0, 0, 640, 480,
      OP.Bind, 2, 0, KIND.Texture, albedo, 0,
      OP.Bind, 2, 1, KIND.Sampler, sampler, 0,
      OP.Bind, 1, 0, KIND.Value, 0, 16,
      OP.DrawIndexed, 6, 1, 0, 0, 0,
      OP.EndPass,
    ];
    assert.equal(await host.gpuSubmit(device, commands, tint), 0, host.gpuLastError());
    const pass = fake.lastPass;
    const [color] = pass.desc.colorAttachments;
    assert.equal(color.loadOp, 'clear');
    assert.ok(Math.abs(color.clearValue.g - 0.2) < 1e-6);
    assert.equal(color.view.view.desc.canvas, 1, 'the frame draws into the canvas texture');
    assert.equal(pass.desc.depthStencilAttachment.depthClearValue, 1);
    assert.deepEqual(pass.calls.map(c => c[0]), [
      'setPipeline', 'setVertexBuffer', 'setIndexBuffer', 'setViewport', 'setScissorRect',
      'setBindGroup', 'setBindGroup', 'drawIndexed', 'end',
    ]);
    assert.equal(pass.calls[2][2], 'uint32');
    assert.deepEqual(pass.calls[3].slice(1), [0, 0, 640, 480, 0, 1]);
    assert.deepEqual(pass.calls[7].slice(1), [6, 1, 0, 0, 0]);
    const uniform = pass.calls[6][2].group.entries[0].resource.buffer;
    assert.deepEqual([...new Float32Array(uniform.data.buffer)], [1, 0.5, 0.25, 1]);

    assert.equal(await host.gpuSurfacePresent(surface), 0);
    assert.equal(host.gpuTextureWrite(frame, 0, 0, []), 1, 'the presented frame handle is gone');
    assert.match(host.gpuLastError(), /^EINVALID\n/);
    const next = await host.gpuSurfaceAcquire(surface);
    assert.ok(next > frame);
    assert.equal(canvas.frames, 2);

    assert.equal(host.gpuSurfaceCreate(device, 9999, true), 0);
    assert.match(host.gpuLastError(), /^ESURFACE\n/);
    host.gpuRelease(surface);
    assert.equal(canvas.configured, null);
    assert.equal(await host.gpuSurfaceAcquire(surface), 0);
  } finally { uninstall(); }
});

test('resources retain their device until the final resource is released', async () => {
  const host = makeGpuHost();
  const { device, fake } = await openDevice(host);
  try {
    const buffer = host.gpuBufferCreate(device, 16, 1);
    const texture = host.gpuTextureCreate(device, [4, 4, 1, 1, 0, 0, 1]);
    host.gpuRelease(buffer);
    assert.equal(fake.buffers[0].destroyed, true);
    assert.equal(host.gpuBufferWrite(buffer, 0, [1, 2, 3, 4]), 1);
    assert.match(host.gpuLastError(), /^EINVALID\nthe buffer was released/);
    host.gpuRelease(buffer);
    host.gpuRelease(device);
    assert.equal(fake.destroyed, false);
    assert.equal(host.gpuTextureWrite(texture, 0, 0, new Array(64).fill(0)), 0);
    assert.equal(host.gpuDeviceCheck(device), 1);
    assert.match(host.gpuLastError(), /^EINVALID\n/);
    host.gpuRelease(texture);
    assert.equal(fake.destroyed, true);
  } finally { uninstall(); }
});

test('a lost device reports DeviceLost everywhere', async () => {
  const host = makeGpuHost();
  const { device, fake } = await openDevice(host);
  try {
    const buffer = host.gpuBufferCreate(device, 16, 1);
    fake.loseWith('the GPU process crashed');
    await new Promise(resolve => setTimeout(resolve, 0));
    assert.equal(host.gpuDeviceCheck(device), 1);
    assert.equal(host.gpuLastError(), 'EDEVICELOST\nthe GPU process crashed');
    assert.equal(host.gpuBufferCreate(device, 16, 1), 0);
    assert.equal(host.gpuLastError(), 'EDEVICELOST\nthe GPU process crashed');
    assert.equal(await host.gpuSubmit(device, [], []), 1);
    assert.match(host.gpuLastError(), /^EDEVICELOST\n/);
    assert.deepEqual(await host.gpuBufferRead(buffer, 0, 4), []);
    assert.match(host.gpuLastError(), /^EDEVICELOST\n/);
  } finally { uninstall(); }
});

test('features WebGPU lacks report UnsupportedCapability with the GpuFeature number', async () => {
  const host = makeGpuHost();
  const { device } = await openDevice(host);
  try {
    assert.equal(host.gpuRayTracingPipelineCreate(device, [], '', []), 0);
    assert.equal(host.gpuLastError(), 'EUNSUPPORTED\n0');
    assert.equal(await host.gpuBottomLevelBuild(device, []), 0);
    assert.equal(host.gpuLastError(), 'EUNSUPPORTED\n0');
    assert.equal(await host.gpuTopLevelBuild(device, [], []), 0);
    assert.equal(host.gpuMlpUpload(device, [], [1, 1, 2]), 0);
    assert.equal(host.gpuLastError(), 'EUNSUPPORTED\n4');
    assert.equal(host.gpuDescriptorHandle(1), 0xffffffffffffffffn);
    assert.equal(host.gpuLastError(), 'EUNSUPPORTED\n2');
    const scene = { ...SAXPY, name: 'trace', params: [{ name: 'scene', kind: KIND.AccelerationStructure, binding: 0 }] };
    assert.equal(host.gpuComputePipelineCreate(host.gpuLibraryLoad(device, wgsl([scene])), 'trace'), 0);
    assert.equal(host.gpuLastError(), 'EUNSUPPORTED\n1');
    const plain = { ...SAXPY, name: 'plain', code: SAXPY_WGSL.replace('fn saxpy', 'fn plain') };
    const needsSubgroups = host.gpuLibraryLoad(device, wgsl([{ ...SAXPY, capabilities: ['subgroup_basic'] }, plain]));
    assert.equal(host.gpuComputePipelineCreate(needsSubgroups, 'saxpy'), 0);
    assert.equal(host.gpuLastError(), 'EUNSUPPORTED\n3');
    // Requirements are per entry: the module's other shaders still run.
    assert.ok(host.gpuComputePipelineCreate(needsSubgroups, 'plain') > 0, host.gpuLastError());
    const needsHalf = host.gpuLibraryLoad(device, wgsl([{ ...SAXPY, capabilities: ['half'] }]));
    assert.equal(host.gpuComputePipelineCreate(needsHalf, 'saxpy'), 0);
    assert.equal(host.gpuLastError(), 'EUNSUPPORTED\n6');
    const unknown = host.gpuLibraryLoad(device, wgsl([{ ...SAXPY, capabilities: ['wave_ops'] }]));
    assert.equal(host.gpuComputePipelineCreate(unknown, 'saxpy'), 0);
    assert.match(host.gpuLastError(), /^EINVALID\nsaxpy needs the unknown capability wave_ops/);
  } finally { uninstall(); }
  const granted = await openDevice(host, ['subgroups', 'shader-f16']);
  try {
    const library = host.gpuLibraryLoad(granted.device, wgsl([{ ...SAXPY, capabilities: ['subgroup_basic'] }]));
    assert.ok(host.gpuComputePipelineCreate(library, 'saxpy') > 0, host.gpuLastError());
    const half = host.gpuLibraryLoad(granted.device, wgsl([{ ...SAXPY, capabilities: ['half'] }]));
    assert.ok(host.gpuComputePipelineCreate(half, 'saxpy') > 0, host.gpuLastError());
  } finally { uninstall(); }
});

// ----- real generator output (tests/gpu_shaders.rs builds it and sets DREAM_GPU_BUNDLES) -----

const WGSL_STAGES = { compute: 'compute', vertex: 'vertex', fragment: 'fragment' };

// Size and alignment of a WGSL type in the uniform address space, for the types the generator
// writes into uniform blocks.
function wgslLayoutOf(code, type) {
  const scalar = { f32: 4, i32: 4, u32: 4, f16: 2, bool: 4 };
  type = type.trim();
  if (scalar[type]) return { size: scalar[type], align: scalar[type] };
  let m = /^vec([234])<(\w+)>$/.exec(type);
  if (m) {
    const n = Number(m[1]);
    const s = scalar[m[2]];
    return { size: n * s, align: (n === 3 ? 4 : n) * s };
  }
  m = /^array<(.+),\s*(?:i32\()?(\d+)\)?>$/.exec(type);
  if (m) {
    const element = wgslLayoutOf(code, m[1]);
    const stride = Math.ceil(element.size / element.align) * element.align;
    return { size: stride * Number(m[2]), align: Math.max(element.align, 16) };
  }
  const fields = wgslStructFields(code, type);
  const align = Math.max(16, ...fields.map(f => f.align));
  const end = fields.length ? fields.at(-1).offset + fields.at(-1).size : 0;
  return { size: Math.ceil(end / align) * align, align };
}

function wgslStructFields(code, name) {
  const body = new RegExp('struct\\s+' + name + '\\s*\\{([^}]*)\\}').exec(code);
  assert.ok(body, 'WGSL declares struct ' + name);
  const declarations = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < body[1].length; i++) {
    const c = body[1][i];
    if (c === '<' || c === '(') depth++;
    else if (c === '>' || c === ')') depth--;
    else if (c === ',' && depth === 0) {
      declarations.push(body[1].slice(start, i));
      start = i + 1;
    }
  }
  declarations.push(body[1].slice(start));
  const fields = [];
  let cursor = 0;
  for (const declaration of declarations.filter(d => d.trim())) {
    const m = /^\s*(?:@align\((\d+)\)\s*)?(\w+)\s*:\s*([\s\S]+?)\s*$/.exec(declaration);
    assert.ok(m, 'unparsed WGSL field `' + declaration.trim() + '` in ' + name);
    const layout = wgslLayoutOf(code, m[3]);
    const align = m[1] ? Number(m[1]) : layout.align;
    const offset = Math.ceil(cursor / align) * align;
    fields.push({ name: m[2], offset, size: layout.size, align });
    cursor = offset + layout.size;
  }
  return fields;
}

// The bind group layout entry the parameter table and the WGSL declaration call for.
function expectedLayoutEntry(param, decl) {
  if (param.kind === KIND.Buffer) {
    assert.equal(decl.space, (param.flags & 1) ? '<storage,read>' : '<storage,read_write>', param.name);
    return 'buffer';
  }
  if (param.kind === KIND.Value) {
    assert.equal(decl.space, '<uniform>', param.name);
    return 'buffer';
  }
  if (param.kind === KIND.Texture) {
    assert.match(decl.type, /^texture_(?!storage)/, param.name);
    return 'texture';
  }
  if (param.kind === KIND.Sampler) {
    assert.match(decl.type, /^sampler/, param.name);
    return 'sampler';
  }
  if (param.kind === KIND.StorageTexture) {
    assert.match(decl.type, /^texture_storage/, param.name);
    return 'storageTexture';
  }
  assert.fail('parameter ' + param.name + ' of kind ' + param.kind + ' cannot appear in a wgsl target');
}

function checkLayouts(fake, entries, before) {
  const layouts = fake.named('createBindGroupLayout').slice(before).flatMap(l => l.layout.entries);
  for (const { entry, decls } of entries) {
    for (const param of entry.params) {
      const decl = decls.get(param.group + ':' + param.binding);
      const key = expectedLayoutEntry(param, decl);
      assert.deepEqual(param.webgpuLayout, fixtureLayout(param), 'generated descriptor: ' + param.name);
      const found = layouts.find(l => l.binding === param.binding && l[key]);
      assert.ok(found, entry.name + '.' + param.name + ' has no ' + key + ' layout entry at binding ' + param.binding);
    }
  }
}

const realBundles = process.env.DREAM_GPU_BUNDLES;
test('generator bundles translate into WebGPU pipelines', {
  skip: realBundles ? false : 'set DREAM_GPU_BUNDLES to a directory of generated .bundle files',
}, async () => {
  const files = readdirSync(realBundles).filter(f => f.endsWith('.bundle')).sort();
  assert.ok(files.length > 0, 'no .bundle files in ' + realBundles);
  for (const file of files) {
    const bytes = [...readFileSync(join(realBundles, file))];
    const parsed = gpuParseBundle(bytes);
    assert.deepEqual(parsed.targets.map(t => t.name), ['wgsl'], file + ': wasm bundles carry WGSL only');
    const [target] = parsed.targets;
    assert.deepEqual(target.layouts, [], file + ': wasm bundles omit the layouts');
    const checked = [];
    for (const entry of target.entries) {
      const code = new TextDecoder().decode(entry.code.bytes);
      const stage = WGSL_STAGES[entry.stage];
      assert.ok(stage, entry.name + ': stage ' + entry.stage + ' has no WGSL form');
      assert.match(code, new RegExp('@' + stage + '\\b[^{;]*?\\bfn\\s+' + entry.name + '\\s*\\('),
        file + ': the WGSL entry point must keep the Dream name ' + entry.name);
      const decls = gpuScanWgslBindings(code);
      for (const param of entry.params) {
        assert.ok(decls.has(param.group + ':' + param.binding),
          entry.name + '.' + param.name + ' has no WGSL declaration at @group(' + param.group + ') @binding(' + param.binding + ')');
      }
      const values = entry.params.filter(p => p.kind === KIND.Value);
      if (values.length) {
        assert.ok(values.every(v => v.group === values[0].group && v.binding === values[0].binding),
          entry.name + ': value parameters share one uniform block');
        const block = decls.get(values[0].group + ':' + values[0].binding);
        const fields = wgslStructFields(code, block.type);
        assert.equal(fields.length, values.length, entry.name + ': one uniform field per value parameter');
        values.forEach((value, i) => {
          assert.ok(fields[i].name.startsWith(value.name + '_'), entry.name + ': field ' + fields[i].name + ' holds ' + value.name);
          assert.equal(value.offset, fields[i].offset, entry.name + '.' + value.name + ' offset');
          assert.equal(value.size, fields[i].size, entry.name + '.' + value.name + ' size');
        });
      }
      checked.push({ entry, decls, values });
    }

    const host = makeGpuHost();
    const { device, fake } = await openDevice(host, ['subgroups', 'shader-f16']);
    try {
      const library = host.gpuLibraryLoad(device, bytes);
      assert.ok(library > 0, host.gpuLastError());
      for (const item of checked.filter(c => c.entry.stage === 'compute')) {
        const before = fake.named('createBindGroupLayout').length;
        const pipeline = host.gpuComputePipelineCreate(library, item.entry.name);
        assert.ok(pipeline > 0, item.entry.name + ': ' + host.gpuLastError());
        assert.equal(fake.named('createComputePipeline').at(-1).compute.compute.entryPoint, item.entry.name);
        checkLayouts(fake, [item], before);

        // Bind every parameter; each value gets a distinct byte pattern that must land at its
        // reflected offset inside the uniform block.
        const words = [OP.BeginComputePass, OP.SetPipeline, pipeline];
        const data = [];
        item.entry.params.forEach((param, index) => {
          if (param.kind === KIND.Value) {
            words.push(OP.Bind, 0, index, KIND.Value, data.length, param.size);
            for (let b = 0; b < param.size; b++) data.push((index * 16 + b + 1) & 255);
          } else if (param.kind === KIND.Buffer) {
            words.push(OP.Bind, 0, index, KIND.Buffer, host.gpuBufferCreate(device, 256, 1), 0);
          }
        });
        words.push(OP.Dispatch, 1, 1, 1, OP.EndPass);
        assert.equal(await host.gpuSubmit(device, words, data), 0, item.entry.name + ': ' + host.gpuLastError());
        if (item.values.length) {
          const group = fake.lastPass.calls.find(c => c[0] === 'setBindGroup')[2].group;
          const uniform = group.entries.find(e => e.binding === item.values[0].binding).resource.buffer;
          item.entry.params.forEach((param, index) => {
            if (param.kind !== KIND.Value) return;
            const got = [...uniform.data.subarray(param.offset, param.offset + param.size)];
            const want = Array.from({ length: param.size }, (_, b) => (index * 16 + b + 1) & 255);
            assert.deepEqual(got, want, item.entry.name + '.' + param.name + ' bytes in the uniform block');
          });
        }
      }
      const vertices = checked.filter(c => c.entry.stage === 'vertex');
      const fragments = checked.filter(c => c.entry.stage === 'fragment');
      for (const vertex of vertices) {
        for (const fragment of fragments) {
          const before = fake.named('createBindGroupLayout').length;
          const pipeline = host.gpuRenderPipelineCreate(library, vertex.entry.name, library, fragment.entry.name,
            [0, 0, 0, 0, 0, 1, 1, 1, 0]);
          assert.ok(pipeline > 0, vertex.entry.name + '/' + fragment.entry.name + ': ' + host.gpuLastError());
          const { render } = fake.named('createRenderPipeline').at(-1);
          assert.equal(render.vertex.entryPoint, vertex.entry.name);
          assert.equal(render.fragment.entryPoint, fragment.entry.name);
          checkLayouts(fake, [vertex, fragment], before);
        }
      }
    } finally { uninstall(); }
  }
});
