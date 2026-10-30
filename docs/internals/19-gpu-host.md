# Native GPU host

`system.gpu` runs natively through the `dream-host-gpu` capability library (`HostCapability::Gpu`,
the `gpu` feature of `dream-host`). It drives [slang-rhi](https://github.com/shader-slang/slang-rhi)
and loads only the precompiled shader bundles described in
[GPU shader bundle](18-gpu-shader-bundle.md). The Slang compiler is never linked or loaded.

## Crate layout

| Path | Role |
|---|---|
| `build.rs`, `build/fetch.rs` | Downloads the pinned inputs, applies `patches/`, builds `native/` with CMake |
| `patches/` | slang-rhi patches: MSL source as precompiled Metal code; unbound acceleration structure slots left unwritten on Vulkan devices without `nullDescriptor`; D3D12 descriptor reclamation and allocation-failure cleanup |
| `pins.toml` | URLs and SHA-256 of Slang, slang-rhi, Dawn, Vulkan-Headers, metal-cpp, WinPixEventRuntime |
| `native/replay/` | The replay library: answers slang-rhi's reflection queries from a recorded layout |
| `native/recorder/` | `dream-slang-layout`, the build-time recorder that writes those layouts |
| `native/glue/` | `dgpu.h`, the flat C surface over slang-rhi that the Rust code calls |
| `src/` | Handle table, bundle reader, backend selection, command decoding, futures |
| `aot_probe/` | The Milestone 0 proof, kept runnable with `python3 aot_probe/build.py` |
| `tests/backends.rs` | The backend matrix (one child process per backend) |
| `tests/fixtures/` | Test bundles and `make_bundles.py`, which writes them with the pinned tools |

## Build

`build.rs` downloads each archive listed in `pins.toml` once into the GPU cache, verifies its
SHA-256, and extracts it. The cache is `$DREAM_GPU_CACHE`, or `<target>/gpu-cache`. Offline
builds work once the cache holds the archives; a failed download names the file and the path it
belongs at. The steps are:

1. Read `include/` from the host's Slang release, or from `$DREAM_SLANG` when that root carries
   the pinned version's headers. Nothing else in the release is used.
2. Copy slang-rhi and apply `patches/*.patch` with `git apply`. `GIT_CEILING_DIRECTORIES` keeps
   git away from the Dream checkout above the cache.
3. Configure `native/CMakeLists.txt`. Dawn, Vulkan-Headers, metal-cpp and WinPixEventRuntime reach
   slang-rhi as verified `file://` URLs with their SHA-256, so CMake downloads nothing. DXC, OptiX,
   NVAPI, the Agility SDK and Aftermath are disabled. The replay library is slang-rhi's `slang`
   target (`SLANG_RHI_BUILD_FROM_SLANG_REPO=ON`).
4. Link the static archives (listed in `archives.txt`) into `libdream_host_gpu`. Copy Dawn's shared
   library into the profile directory, because slang-rhi loads Dawn from the directory of the
   library that links it. `HostCapability::companion_library_names` makes the compiler's runtime
   staging and `dreamer pack` ship Dawn next to the host library.

Downloaded archives and verified source trees are shared in the GPU cache. CMake build directories,
including FetchContent's dependency subbuilds, stay under Cargo's `OUT_DIR`. Generator and CMake
toolchain environment settings select separate directories, so Ninja and Visual Studio never reuse
one another's `CMakeCache.txt`. The build respects `CMAKE_GENERATOR` and otherwise uses CMake's
platform default; installing Ninja on `PATH` does not silently change that choice.

The build needs CMake, a C++20 compiler and git. Building the backends needs no SDK beyond the
platform's: Metal, Vulkan and WebGPU on macOS; Vulkan, CUDA and WebGPU on Linux; D3D12, D3D11,
Vulkan, CUDA and WebGPU on Windows. Vulkan and CUDA drivers are loaded at run time. On macOS,
Vulkan needs the Khronos loader in front of MoltenVK, because MoltenVK alone rejects
`VK_KHR_portability_enumeration`.

## Manual hardware qualification

`python scripts/qualify-gpu.py --backend dx12` (or `vulkan`) runs the existing compute, bindless,
ray-query and ray-pipeline assertions on that backend alone. `--backend cuda` runs the compute
assertions. The command preserves its project and log under `target/gpu-qualification/` and
fails when a requested check skips, so missing drivers cannot count as hardware qualification.

On Windows, install the CUDA toolkit and make its `bin` directory available through `PATH` and
`CUDA_PATH` for the CUDA host. Copy the toolkit's `nvrtc*.dll` files, including the builtins DLL,
into the Slang toolchain's `bin` directory so the tool sandbox can compile PTX. Windows
qualification used NVRTC/NVCC 12.9.86 with runtime headers 12.9.79. CUDA cooperative vectors
are unavailable in the pinned build without OptiX.

The CUDA host uses NVRTC for slang-rhi's internal texture-clear kernels at device creation;
Dream's shader entry points are precompiled PTX. CUDA does not have the same compiler-free
runtime dependency set as Vulkan, D3D12 or Metal.

## `dream-slang-layout`

`dreamer toolchain install slang` extracts the Slang release, then builds
`<root>/bin/dream-slang-layout` from `native/recorder/` and `native/replay/layout_blob.*`. Those
sources are embedded in `dreamer`. The build links the release's `slang-compiler` library, and the
host C++ compiler is found the way Cargo build scripts find it (the `cc` crate). The tool must link
the exact compiler it reflects, so it is built at install time rather than shipped. It is the
second executable of the `slang` pinned tool, so generators reach it with
`@uses_tool("dream-slang-layout")`.

Re-running `dreamer toolchain install slang` refreshes the recorder when its embedded C++
sources or build instructions change, even if the Slang release is already installed. The
source fingerprint is written only after a successful build.

The tool takes slangc's options: positional `.slang` modules, `-entry` with an optional `-stage`,
and `-target`. The last `-o` names the layout. An earlier `-o` right after an entry writes that
entry's code from the same linked program, which is useful when one run must produce code and
layout together. `-reflection-json` is accepted and ignored.

## Threading and futures

- One process-wide lock guards the handle table and every glue call that touches a device.
- Each device owns a completion thread, reached through the shared device core rather than the
  device's handle, so reads and submissions on live resources work after the guest released the
  device itself. A submission returns a fence value; the thread waits for it without the lock,
  then takes the lock to finish the job and resolves the guest future with
  `dream_host_complete_foreign`. Queued jobs keep the device alive. Every async import is
  `@async_host`.
- Reads allocate their guest byte array on the guest thread when they start. Completion threads
  only fill it, because they must not allocate from the guest's thread-local arena. A failed read
  resolves to the same array with its length set to 0.
- Device creation runs on a short-lived thread, because creating a device can take long enough to
  stall a frame.
- Surface creation, acquire and present run on the guest thread, which is the process main
  thread. They complete their futures immediately.

## Resources

Handles come from `dream_host_abi::handles::HandleTable`: they are positive, `0` means failure,
and a released slot comes back with a new generation, so a stale handle misses instead of naming
a newer resource and the id space never runs out. The guest keeps each handle in a
`GpuOwnedHandle`, and command encoders and `GpuArgs` keep the owned handles of every resource
their commands name, so a resource whose last user reference is gone is released only after
`submit` handed its commands to the host. Each entry records its owning device,
and using a resource with another device reports `EWRONGDEVICE`. Each submission stamps every
resource it references with its fence value. `gpuRelease` frees a resource at once when its last
submission has finished. Otherwise the resource moves to a deferred list, which the completion
thread drains as fences complete. Devices are reference-counted by their resources and their
completion thread, so a device is destroyed only after all of its work and resources are gone.

## Backend selection

`backend::select` is pure and unit-tested:

- `Auto` tries, in order, Metal, Vulkan, WebGPU on macOS; D3D12, Vulkan, D3D11, WebGPU on
  Windows; Vulkan, WebGPU elsewhere. It skips backends missing from the guest's compiled mask, so
  a program without shaders (mask `0`) may open any of them. CUDA runs compute only, so `Auto`
  never picks it.
- An explicit backend that is not in the compiled mask reports `ENOTCOMPILED` with the backend
  number. One that cannot open (no adapter or driver, or a host built without it) reports
  `EUNAVAILABLE` with the reason. Neither ever falls back to another backend.
- Cooperative vectors (`GpuFeature.CooperativeVectors`) are reported only on Vulkan, D3D12 and
  CUDA devices whose slang-rhi `Feature::CooperativeVector` is present. The pinned build leaves
  out NVAPI and OptiX, so in practice only Vulkan devices with `VK_NV_cooperative_vector` report it.
- slang-rhi's Metal device reports neither wave operations nor half precision, which every GPU it
  opens has, so Metal devices report `Subgroups` and `Float16` themselves. It never reports
  cooperative matrices either; Metal devices in the Apple7 family or later (Apple silicon) report
  `CooperativeMatrices`, because Slang lowers 8x8 `CoopMat` to `simdgroup_matrix`. Bindless
  handles exist only where slang-rhi implements descriptor handles (Vulkan, D3D12, CUDA).
- Creating a pipeline checks the bundle capabilities of its entries (see
  [GPU shader bundle](18-gpu-shader-bundle.md)) and reports `EUNSUPPORTED` with the missing
  feature.

## Buffers and MLP layers

- Buffers are allocated in whole 4-byte words on every backend, because WebGPU sizes buffers,
  fills and copies that way. Reads copy whole words into the padding. A write that ends the
  buffer is zero-padded into the padding; on WebGPU any other write must start and end on a
  word boundary (`EINVALID` otherwise). The browser host follows the same rules.
- `gpuMlpUpload` (`src/mlp.rs`) stores one layer as biases at offset 0 and the matrix at the
  next multiple of 64 bytes, the layout the generated `coopVecMatMulAdd` reads (see
  [GPU shader bundle](18-gpu-shader-bundle.md)). `mlp::layout_for` picks the matrix layout. On
  Vulkan and D3D12 devices with cooperative vectors it is the inferencing-optimal layout, which
  `dgpu_mlp_convert_optimal` produces on the CPU through slang-rhi's
  `getCooperativeVectorMatrixSize` and `convertCooperativeVectorMatrix`. Everything else keeps
  row-major weights: CUDA, because its PTX is compiled without OptiX and Slang emulates the
  multiply on row-major data, and devices without the feature, which cannot run the code anyway.

## Bindings

A pipeline's program contains exactly the entries it uses, which come from one pipeline group
(see [GPU shader bundle](18-gpu-shader-bundle.md)). The replay library re-points the group
layout's entry list at them by name (`createPrecompiledProgram`). Each bundle parameter
row is resolved once, at pipeline creation, to a slang-rhi cursor path. The rules, in priority
order:

1. A field with the parameter's name and a matching binding.
2. A field with the parameter's name.
3. A field with a matching binding number.

Only fields that can hold the parameter's kind are candidates: samplers match sampler fields,
textures match texture-shaped resources, and so on. Binding numbers alone are not unique, because
Metal numbers buffers, textures and samplers in separate index spaces. The entry's scope is
searched before the globals, because reflection reports entry-point bindings relative to the
entry. A value parameter matches the uniform block it was packed into, and its
bytes land at the recorded offset inside that block. WebGPU rejects a bind group with an empty
slot, while the other backends ignore slots the code never reads. Because a render or ray-tracing
group's layout covers the globals of all its shaders, the glue fills every slot with per-device placeholders right
after binding a WebGPU pipeline, and the real bindings then overwrite them. Read-only slots share
one placeholder per kind; each writable slot gets its own, because WebGPU rejects one buffer or
texture bound to two writable slots.

## Ray tracing

`src/raytracing.rs` decodes and validates the guest's encodings; the glue
(`native/glue/raytracing.cpp`) only translates them.

- `gpuBottomLevelBuild` builds triangles (`GpuVec3` positions, 12-byte stride, optional 32-bit
  indices) or boxes (min/max `GpuVec3` pairs) into an acceleration structure. `gpuTopLevelBuild`
  converts each instance to the device's native instance record (D3D12, Vulkan, Metal or OptiX)
  with its 3x4 row-major transform, id, mask, hit group offset and flags. Both record the build
  into their own submission, insert the handle at once, and resolve the future with it when the
  fence completes; scratch and instance buffers live until then. A top-level structure keeps its
  bottom levels alive, and only top-level structures bind to shaders.
- `gpuRayTracingPipelineCreate` decodes the `GpuDevice.ray_tracing_pipeline` layout into the
  shader binding table: one ray generation shader, misses, hit groups named `hit_group_<i>`
  (closest hit, optional any hit and intersection), and callables. All shaders must come from
  one shader module. The program lists ray generation first and each shader once; only ray
  generation takes parameters. Pipelines reserve a 128-byte payload, 32-byte hit attributes and a
  recursion depth of 1 (rays traced from ray generation only).
- The command stream traces inside a compute pass (`SetPipeline`, `Bind` with stage
  `RayGeneration`, `TraceRays`); binding a ray tracing pipeline turns the pass into a slang-rhi
  ray tracing pass.
- Acceleration structures need `GpuFeature.RayTracing` or `GpuFeature.RayQuery`; ray tracing
  pipelines need `RayTracing`. Buffers on devices with acceleration structures may feed builds.
  Native render passes bind top-level structures through the acceleration-structure overload
  of `GpuRenderPass.bind`; fragment shaders can issue inline ray queries directly.

## Strict ahead-of-time check

With `DREAM_GPU_STRICT_AOT=1` the loader check becomes an assertion: once a Slang compiler image
is mapped, the next device creation, bundle load or pipeline creation ends the process with the
image's path. `scripts/test_corpus.py` sets it and removes `DREAM_SLANG` for every run.

## Errors

The glue returns a status and a thread-local message. Driver messages reach a per-device sink,
which keeps the first error since the last failure was reported: later errors are usually
consequences of it, such as Dawn's "invalid pipeline" after a pipeline creation failed. The Rust side maps these to the
`CODE\ndetail` text that `gpuLastError` returns (see `gpu_error_from_host`). Driver messages that
report device loss make the device sticky-lost, and every later call on it reports `EDEVICELOST`.
`testing::simulate_device_loss` drives the same path in tests.

Recording can fail through slang-rhi's debug callback even when `finish` succeeds. Submission
checks the sink before sending the command buffer, so failed binding allocation cannot silently
drop draw state. The D3D12 patch retires completed command buffers before a new encoder reserves
descriptors and excludes failed arena allocations from later frees. Its regression fills the
sampler heap, expects a recoverable error, then verifies subsequent valid submissions succeed.

## Tests

- `cargo test -p dream-host-gpu --lib` covers:
  - the bundle reader, including property tests that feed it arbitrary and mutated bytes
  - the command stream decoder, with fuzzing
  - backend selection
  - the handle table
  - render pipeline description decoding
- `cargo test -p dream-host-gpu --test backends` runs device-free selection and validation
  checks, then re-runs itself once per backend. Each backend dispatches compute in all three
  binding styles (globals plus a parameter block, entry-point parameters, and the generator's
  `<entry>_<param>` globals). It also renders an offscreen triangle and a textured, indexed quad
  with a depth attachment, reads them back, checks error mapping and simulated device loss, and
  asserts that no Slang compiler image was mapped. A
  backend that the host or machine cannot run is reported as skipped. On macOS the Vulkan child
  uses the Khronos loader and MoltenVK from the GPU cache unless `VK_DRIVER_FILES` is set.
- `DREAM_GPU_SURFACE_TEST=1` adds a real window from `dream-host-window` and presents three frames
  to it. When `DREAM_SLANG` (or `~/.dream/toolchains/slang-2026.20`) is present, a loader
  self-test maps the Slang compiler on purpose to prove that the check detects it.
- Regenerate the fixtures with `python3 tests/fixtures/make_bundles.py`. The output is
  deterministic.
- `cargo test -p dream-host-gpu --lib` also unit-tests the ray tracing decoders (geometry,
  instance records and transforms, binding table layout and program order, with fuzzing). The
  backend test's strict-AOT self-test maps the Slang compiler with `DREAM_GPU_STRICT_AOT=1` and
  checks that device creation ends the process.
- The Dream suites in `tests/projects/gpu_runtime/` run real programs through the generator and
  this host on every compiled backend: compute, workgroup and subgroup reductions, an offscreen
  render read back pixel by pixel, bindless sampling, inline ray queries, a ray tracing pipeline,
  backend selection, forward- and reverse-mode autodiff checked against finite differences,
  `float16` buffers and half arithmetic, and cooperative vectors and matrices (which also check
  that devices without them report `UnsupportedCapability`). A backend the machine cannot run prints `SKIP <test> on <backend>:
  <reason>` and the test continues, so machines without a GPU pass. On macOS the corpus script
  points Vulkan at the GPU cache's Khronos loader and MoltenVK.

## Not yet implemented

- Ray queries and ray tracing pipelines were qualified on D3D12 and Vulkan on an RTX 4080
  under Windows on 2026-10-19 with `scripts/qualify-gpu.py`. Metal has no ray tracing
  passes and MoltenVK has no ray tracing extensions, so macOS uses inline queries on Metal.
  The pipeline suite skips on devices without `GpuFeature.RayTracing`.
- Payload size, hit attribute size and recursion depth are fixed (128 bytes, 32 bytes, 1); the
  bundle does not record them yet.
- The inferencing-optimal MLP conversion has not run on hardware: no available machine has a
  Vulkan or D3D12 device with cooperative vectors.
- A render or ray tracing pipeline binds the globals of all of its module's shaders of those
  stages, because they share one recorded layout. On WebGPU, their storage buffers together
  count toward the per-stage limit (8 by default, 10 on Dawn over Metal); the generator warns
  past 8. Compute pipelines bind only their own shader's globals.
- Vertex and fragment shaders of one pipeline must come from the same shader module.
- Vertex attribute locations must be dense (`0..n-1`): slang-rhi's Vulkan, Metal and WebGPU
  backends take attribute `i` as shader location `i`. D3D binds by the generator's `LOCATION<n>`
  semantics.
- Wayland windows have no slang-rhi surface. GLFW runs them through XWayland, which gives an Xlib
  handle.

## Native drawing handoff

On Windows, `drawingCopyGpu` discovers `dream_gpu_drawing_copy_v1` in the already-loaded GPU
capability. The ABI in `dream-host-abi::drawing_gpu` passes a synchronous producer callback;
neither capability owns or statically links the other's handle table. The GPU state lock is
held through the handoff, and the producer never enters the GPU host or guest scheduler again.

D3D12 destinations on the same adapter use one cached shared texture and shared timeline fence
per GPU device. The producer imports their handles into wgpu, waits on the previous consumed
value, copies or blits the current drawing, transitions the shared texture back to COMMON, and
signals ready. The consumer waits for ready on its GPU queue, copies into mip/layer zero,
restores default resource states, and signals consumed plus its normal submission fence.
These are GPU waits; neither side maps a pixel buffer or waits for completion on the CPU.

A transfer uses values `2n` (previously consumed), `2n+1` (ready), and `2n+2` (consumed).
The imported wrapper is keyed by a monotonic generation, not a recycled OS handle. Failed
handoffs invalidate the cached export so an old ready signal cannot authorize a later copy.
The destination's last-use fence keeps it alive after guest release, and an internal completion
job collects deferred resources without completing a guest future.

The shared texture is linear RGBA8 or BGRA8; sRGB variants reuse the same family. Copies preserve
stored bytes rather than decoding/encoding sRGB again. BGRA uses an exact texture-load shader
on the producer GPU. Each target is bounded by drawing's 8192-side/16-megapixel limits. Size or
family changes while the prior handoff is still executing return an allocation error instead
of allocating another full-surface target. Unsupported backends and adapter mismatches use the
existing pixel bridge. D3D12's global debug layer must not be enabled after another capability
has opened a device, so drawing disables that instance flag while retaining wgpu validation.

Run the hardware test with:

```powershell
$env:DREAM_DRAWING_BENCH = '1'
cargo test -p dream-host-drawing dx12_drawing -- --ignored --nocapture
```

It checks all four RGBA/BGRA linear/sRGB formats, repeated changing images, zero drawing pixel
reads, target reuse, invalid/released targets, and early drawing/device release. The optional
benchmark reports 30 warm transfers at 320×240. A local Windows run measured 564.7 µs/frame for
the previous readback/upload path (including its waits), and 244.5 µs/frame for GPU-only
submission. Direct rendering created one shared target, no new warm targets, and zero drawing
pixel reads. These are calling-thread wall times, not GPU execution times or CI budgets;
destination verification reads run outside both timed loops. The target counter excludes RHI,
wgpu, command and driver allocations.
