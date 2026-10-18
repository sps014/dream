# Real browser GPU tests

Run from this directory:

```sh
npm ci
npx playwright install chromium
npm test
```

The test serves Dream's JavaScript hosts and committed shader bundles over localhost. It runs
two compute pipelines with readback, checks every pixel of an offscreen triangle, presents and
resizes a canvas, then destroys the actual WebGPU device and checks device-loss propagation.
Missing WebGPU support fails the test. No GPU calls are mocked.

`GPU_BROWSER_EXECUTABLE` selects an installed Chromium browser, such as Microsoft Edge.
On Linux without a GPU, set `GPU_BROWSER_SOFTWARE=1` to use Chromium's SwiftShader Vulkan
adapter. CI installs Chromium and its system dependencies through Playwright and uses this mode.
Windows qualification uses the default hardware adapter; its Chromium builds do not expose a
SwiftShader WebGPU adapter.

Qualified on Windows with an RTX 4080 in Chromium and Edge on 2026-10-18. Software rendering
exercises the browser integration but does not qualify hardware ray tracing or native GPU hosts.
