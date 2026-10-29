import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve, extname, sep } from 'node:path';
import { spawnSync } from 'node:child_process';
import { chromium } from 'playwright';

const root = fileURLToPath(new URL('../../', import.meta.url));
const output = resolve(root, 'target/drawing-browser/canvas.wasm');
await mkdir(resolve(root, 'target/drawing-browser'), { recursive: true });
const compiler = process.env.DREAM_COMPILER || resolve(root, `target/debug/dream${process.platform === 'win32' ? '.exe' : ''}`);
const compiled = spawnSync(compiler, ['--web', '-o', output, 'tests/fixtures/compiler/drawing_canvas.dream'], { cwd: root, stdio: 'inherit' });
assert.equal(compiled.status, 0, 'compile drawing browser fixture');

const server = createServer(async (request, response) => {
  try {
    const url = new URL(request.url, 'http://localhost');
    if (url.pathname === '/') {
      response.setHeader('Content-Type', 'text/html');
      response.end('<!doctype html><title>Dream drawing test</title><canvas id="drawing" width="32" height="32" style="width:32px;height:32px"></canvas>');
      return;
    }
    const path = resolve(root, '.' + decodeURIComponent(url.pathname));
    if (!path.startsWith(root.endsWith(sep) ? root : root + sep)) {
      response.writeHead(403).end();
      return;
    }
    response.setHeader('Content-Type', extname(path) === '.js' ? 'text/javascript' : extname(path) === '.wasm' ? 'application/wasm' : 'application/octet-stream');
    response.end(await readFile(path));
  } catch { response.writeHead(404).end(); }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
let browser;
const timeout = setTimeout(() => browser?.close(), 120_000);
timeout.unref();
try {
  browser = await chromium.launch({
    channel: 'chromium', headless: process.env.GPU_BROWSER_HEADLESS !== '0',
    executablePath: process.env.GPU_BROWSER_EXECUTABLE || undefined,
    // Mesa WebGPU uses Vulkan; keeping the Xvfb compositor on GL makes canvas screenshots visible.
    args: ['--enable-unsafe-webgpu', ...(process.env.GPU_BROWSER_SOFTWARE === '1' ? ['--use-gl=angle', '--use-angle=gl', '--use-vulkan=native', '--disable-vulkan-surface', '--ignore-gpu-blocklist', '--disable-gpu-sandbox'] : [])],
  });
  const page = await browser.newPage();
  const errors = [];
  const messages = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => messages.push(message.text()));
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  await page.evaluate(() => {
    globalThis.drawingCheckpoint = () => new Promise(resolve => {
      globalThis.drawingResume = resolve;
      console.log('drawing-present-ready');
    });
  });
  const ready = page.waitForEvent('console', { predicate: message => message.text().includes('drawing-present-ready') });
  const execution = page.evaluate(async () => {
    const { run } = await import('/target/drawing-browser/canvas.web.runtime.js');
    await run('/target/drawing-browser/canvas.wasm');
  });
  await Promise.race([ready, execution.then(() => { throw new Error('drawing program ended before presenting'); })]);
  try {
    let pixel;
    const deadline = Date.now() + 10_000;
    do {
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      const screenshot = await page.locator('#drawing').screenshot({ path: resolve(root, 'target/drawing-browser/presentation.png') });
      pixel = await page.evaluate(async bytes => {
        const image = await createImageBitmap(new Blob([new Uint8Array(bytes)], { type: 'image/png' }));
        const canvas = new OffscreenCanvas(image.width, image.height);
        const context = canvas.getContext('2d');
        context.drawImage(image, 0, 0);
        return Array.from(context.getImageData(Math.floor(image.width / 2), Math.floor(image.height / 2), 1, 1).data);
      }, Array.from(screenshot));
      if (pixel.every((channel, index) => channel === [40, 100, 200, 255][index])) break;
    } while (Date.now() < deadline);
    await writeFile(resolve(root, 'target/drawing-browser/presentation.json'), JSON.stringify({ pixel, errors, messages }, null, 2));
    assert.deepEqual(pixel, [40, 100, 200, 255], 'the second cached presentation must reach the window');
  } finally {
    await page.evaluate(() => globalThis.drawingResume());
    await execution;
  }
  assert.deepEqual(errors, []);
  assert(messages.some(message => message.includes('drawing-browser-passed')), messages.join('\n'));
  console.log('Dream CPU/WebGPU backdrop, image and text rendering and cached one-call window presentation passed in Chromium.');
} finally {
  clearTimeout(timeout);
  await browser?.close();
  await new Promise(resolve => server.close(resolve));
}
