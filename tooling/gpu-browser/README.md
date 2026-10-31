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

For `system.drawing`, build the compiler with `cargo build --workspace`, build its portable host
with `python scripts/build-drawing-web.py`, then run `npm run test:drawing`. This compiles a Dream
canvas program and verifies CPU and WebGPU backdrop blur, clipping, images, shaped text and drawing
order in Chromium, followed by cached `DrawingSurface.present` calls through the existing
`GpuSurface` and `GpuFrame` APIs. A screenshot checks the second frame before resources are released.

`GPU_BROWSER_EXECUTABLE` selects an installed Chromium browser, such as Microsoft Edge.
On Linux without a GPU, install Mesa's Vulkan drivers, select its lavapipe ICD with
`VK_ICD_FILENAMES`, and set `GPU_BROWSER_SOFTWARE=1`. CI discovers the installed ICD filename
and installs Chromium and its system dependencies through Playwright.
CI runs a headed browser under Xvfb so canvas presentation has a display. Set
`GPU_BROWSER_HEADLESS=0` to select this mode locally.
Windows qualification uses the default hardware adapter.

Qualified on Windows with an RTX 4080 in Chromium and Edge on 2026-10-18. Software rendering
exercises the browser integration but does not qualify hardware ray tracing or native GPU hosts.

The Linux software suite passes in [CI run 39126271217](https://github.com/sps014/dream/actions/runs/39126271217).

The drawing checks also exercise repeated RGBA/BGRA (including sRGB) GPU copies and device
switches. They instrument `GPUBuffer.mapAsync` and uploads to destination textures while a
transfer runs, so a readback/upload bridge cannot pass as a GPU transfer. Vello's initialization
uploads (such as its alpha lookup texture) remain allowed. Explicit verification reads run outside
these measured sections. Early guest failures are recorded in `target/drawing-browser/failure.json`.
