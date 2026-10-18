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
On Linux without a GPU, install Mesa's Vulkan drivers, select its lavapipe ICD with
`VK_ICD_FILENAMES`, and set `GPU_BROWSER_SOFTWARE=1`. CI discovers the installed ICD filename
and installs Chromium and its system dependencies through Playwright.
CI runs a headed browser under Xvfb so canvas presentation has a display. Set
`GPU_BROWSER_HEADLESS=0` to select this mode locally.
Windows qualification uses the default hardware adapter.

Qualified on Windows with an RTX 4080 in Chromium and Edge on 2026-10-18. Software rendering
exercises the browser integration but does not qualify hardware ray tracing or native GPU hosts.

The Linux software suite passes in [CI run 39126271217](https://github.com/sps014/dream/actions/runs/39126271217).
