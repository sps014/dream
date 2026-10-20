import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve, extname, sep } from 'node:path';
import { chromium } from 'playwright';

const root = fileURLToPath(new URL('../../', import.meta.url));
const server = createServer(async (request, response) => {
  try {
    const url = new URL(request.url, 'http://localhost');
    if (url.pathname === '/') {
      response.setHeader('Content-Type', 'text/html');
      response.end('<canvas id="game" width="64" height="32"></canvas>');
      return;
    }
    const path = resolve(root, '.' + decodeURIComponent(url.pathname));
    if (!path.startsWith(root.endsWith(sep) ? root : root + sep)) {
      response.writeHead(403).end();
      return;
    }
    response.setHeader('Content-Type', extname(path) === '.js' ? 'text/javascript' : 'application/octet-stream');
    response.end(await readFile(path));
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
let browser;
const timeout = setTimeout(() => browser?.close(), 120_000);
timeout.unref();
try {
  browser = await chromium.launch({
    channel: 'chromium',
    headless: process.env.GPU_BROWSER_HEADLESS !== '0',
    executablePath: process.env.GPU_BROWSER_EXECUTABLE || undefined,
    args: [
      '--enable-unsafe-webgpu',
      ...(process.env.GPU_BROWSER_SOFTWARE === '1' ? ['--enable-features=Vulkan', '--use-angle=vulkan', '--use-vulkan=native', '--ignore-gpu-blocklist', '--disable-gpu-sandbox'] : []),
    ],
  });
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  const result = await page.evaluate(async () => {
    const { makeGpuHost } = await import('/runtime/src/hosts/gpu.js');
    const { loadGpuParser } = await import('/runtime/src/hosts/gpu_bundle.js');
    await loadGpuParser('/tests/fixtures/gpu_parser.wasm');
    const { makeWindowHost } = await import('/runtime/src/hosts/window.js');
    const check = (condition, message) => { if (!condition) throw new Error(message); };
    check(navigator.gpu, 'navigator.gpu is missing');
    let nativeDevice;
    let adapterInfo;
    const requestAdapter = navigator.gpu.requestAdapter.bind(navigator.gpu);
    // Capture the actual device so destroying it exercises the browser's device.lost promise.
    navigator.gpu.requestAdapter = async (...args) => {
      const adapter = await requestAdapter(...args);
      check(adapter, 'WebGPU found no adapter');
      adapterInfo = { vendor: adapter.info.vendor, device: adapter.info.device, description: adapter.info.description };
      const requestDevice = adapter.requestDevice.bind(adapter);
      adapter.requestDevice = async (...args) => (nativeDevice = await requestDevice(...args));
      return adapter;
    };
    const host = makeGpuHost();
    const windows = makeWindowHost();
    const ok = (code, what) => check(code === 0, `${what}: ${host.gpuLastError()}`);
    const handle = (value, what) => { check(value > 0, `${what}: ${host.gpuLastError()}`); return value; };
    const device = handle(await host.gpuDeviceCreate(6, 1 << 6), 'device');
    check(typeof host.gpuDeviceAdapterName(device) === 'string', 'adapter label');
    check(host.gpuDeviceAdapterName(-1) === '' && host.gpuLastError().startsWith('EINVALID'), 'invalid adapter handle');
    const bundle = async name => new Uint8Array(await (await fetch(`/crates/dream-host-gpu/tests/fixtures/${name}.dgpb`)).arrayBuffer());
    const library = handle(host.gpuLibraryLoad(device, await bundle('generated')), 'compute library');
    const fill = handle(host.gpuComputePipelineCreate(library, 'fill'), 'fill pipeline');
    const offset = handle(host.gpuComputePipelineCreate(library, 'offset'), 'offset pipeline');
    const buffer = handle(host.gpuBufferCreate(device, 256, 1), 'buffer');
    const values = Array.from(new Uint8Array(new Uint32Array([1000, 48, 7]).buffer));
    ok(await host.gpuSubmit(device, [
      2, 4, fill, 5, 0, 0, 1, buffer, 0, 5, 0, 1, 6, 0, 4, 5, 0, 2, 6, 4, 4, 6, 2, 1, 1, 3,
      2, 4, offset, 5, 0, 0, 1, buffer, 0, 5, 0, 1, 6, 8, 4, 6, 2, 1, 1, 3,
    ], values), 'compute submit');
    const bytes = new Uint8Array(await host.gpuBufferRead(buffer, 0, 256));
    check(bytes.length === 256, 'compute readback length');
    const view = new DataView(bytes.buffer);
    for (let i = 0; i < 64; i++) check(view.getUint32(i * 4, true) === (i < 48 ? 1007 + i : 7), `compute element ${i}`);

    const renderLibrary = handle(host.gpuLibraryLoad(device, await bundle('triangle')), 'render library');
    const pipeline = handle(host.gpuRenderPipelineCreate(renderLibrary, 'triangle_vertex', renderLibrary, 'triangle_fragment', [0, 0, 0, 0, 0, 0, 0, 1, 0]), 'render pipeline');
    const target = handle(host.gpuTextureCreate(device, [64, 32, 1, 1, 0, 1, 1]), 'render target');
    const bits = value => new Int32Array(new Float32Array([value]).buffer)[0];
    const tint = Array.from(new Uint8Array(new Float32Array([0.5, 1, 1, 1]).buffer));
    ok(await host.gpuSubmit(device, [1, target, 1, 0, 0, bits(1), bits(1), 0, 0, 0, 4, pipeline, 5, 2, 0, 6, 0, 16, 11, 3, 1, 0, 0, 3], tint), 'render submit');
    const pixels = await host.gpuTextureRead(target, 0, 0);
    check(pixels.length === 64 * 32 * 4, 'render readback length');
    for (let i = 0; i < pixels.length; i += 4) {
      check(pixels[i] >= 126 && pixels[i] <= 129 && pixels[i + 1] === 0 && pixels[i + 2] === 0 && pixels[i + 3] === 255, `render pixel ${i / 4}: ${pixels.slice(i, i + 4)}`);
    }
    const hdrPipeline = handle(host.gpuRenderPipelineCreate(renderLibrary, 'triangle_vertex', renderLibrary, 'triangle_fragment', [7, 0, 0, 0, 0, 0, 0, 1, 0]), 'HDR pipeline');
    const hdrTarget = handle(host.gpuTextureCreate(device, [64, 32, 1, 1, 7, 1, 1]), 'HDR target');
    const hdrTint = Array.from(new Uint8Array(new Float32Array([4, 2, 0, 1]).buffer));
    ok(await host.gpuSubmit(device, [1, hdrTarget, 1, 0, 0, 0, 0, 0, 0, 0, 4, hdrPipeline, 5, 2, 0, 6, 0, 16, 11, 3, 1, 0, 0, 3], hdrTint), 'HDR render submit');
    const hdrBytes = new Uint8Array(await host.gpuTextureRead(hdrTarget, 0, 0));
    check(hdrBytes.length === 64 * 32 * 8, 'HDR readback length');
    const hdrView = new DataView(hdrBytes.buffer, hdrBytes.byteOffset, hdrBytes.byteLength);
    for (let at = 0; at < hdrBytes.length; at += 8) {
      check(hdrView.getUint16(at, true) === 0x4400 && hdrView.getUint16(at + 6, true) === 0x3c00, 'HDR values above one survive rendering');
    }
    host.gpuRelease(hdrTarget);
    host.gpuRelease(hdrPipeline);
    const window = handle(windows.windowCanvas('game', 0), 'window');
    const surface = handle(host.gpuSurfaceCreate(device, window, true), 'surface');
    for (const [width, height] of [[64, 32], [128, 48]]) {
      ok(host.gpuSurfaceResize(surface, width, height), 'resize');
      const info = host.gpuSurfaceInfo(surface);
      check(info[1] === width && info[2] === height, 'surface dimensions');
      const image = handle(await host.gpuSurfaceAcquire(surface), 'acquire');
      ok(await host.gpuSubmit(device, [1, image, 1, bits(0.25), bits(0.5), bits(0.75), bits(1), 0, 0, 0, 3], []), 'surface submit');
      ok(await host.gpuSurfacePresent(surface), 'present');
    }
    nativeDevice.destroy();
    await nativeDevice.lost;
    await Promise.resolve();
    check(host.gpuDeviceCheck(device) === 1 && host.gpuLastError().startsWith('EDEVICELOST\n'), 'device loss check');
    check(await host.gpuSurfaceAcquire(surface) === 0 && host.gpuLastError().startsWith('EDEVICELOST\n'), 'acquire after loss');
    check(host.gpuBufferCreate(device, 4, 1) === 0 && host.gpuLastError().startsWith('EDEVICELOST\n'), 'buffer after loss');
    for (const value of [surface, target, pipeline, renderLibrary, buffer, fill, offset, library, device]) host.gpuRelease(value);
    windows.windowClose(window);
    return adapterInfo;
  });
  assert.deepEqual(errors, [], 'uncaught browser errors');
  console.log(`WebGPU compute, pixel readback, presentation, resize and device loss passed: ${JSON.stringify(result)}`);
} finally {
  clearTimeout(timeout);
  await browser?.close();
  await new Promise(resolve => server.close(resolve));
}
