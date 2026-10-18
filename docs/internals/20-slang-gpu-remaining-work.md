# Slang GPU support: remaining work

Status of the generator-based Slang GPU work on the `feat/slang-gpu-shaders` branch, and the plan for what is left. The design lives in [GPU shader bundle](18-gpu-shader-bundle.md), [GPU host](19-gpu-host.md) and [Window host](17-window-host.md).

## Landed

| Area | State |
|---|---|
| Generator framework gaps | Data-driven pinned-tool registry (`PINNED_TOOLS`), whole-tree tool fingerprints, per-tool timeouts, module attributes in the snapshot (`modules_with`, `module_of`), `ctx.consume(f, help)`, `@generator_only`, opaque tool outputs with a delta transcript, parallel tool requests, check-only analysis of consumed bodies, LSP hover/definition/code lens inside consumed bodies, binary artifacts, optional tools. |
| `float16` | Primitive type with `h` literals, widening rules, LLVM `half`, wasm32 through f32, shortest round-trip printing, `@json` support. Rejected across `extern`/`@export`. |
| `system.window` | Native (GLFW 3.4, main thread) and browser (`window.js`) hosts, keyboard/mouse/gamepad state, `dream_window_surface_v1` C ABI for GPU surfaces. |
| AOT proof | Slang 2026.20, slang-rhi `b856e635`, Dawn 138.0.7204.168 pinned with SHA-256 and BLAKE3; precompiled code plus a recorded layout load through a replay library, so the Slang compiler is never loaded at run time. |
| `system.gpu` | Device, buffers, textures, samplers, render/compute/ray-tracing pipelines, command encoder, surfaces, errors, shader attributes and `@generator_only` built-ins including ray tracing, bindless, subgroups, autodiff and cooperative types. |
| `system.gpu.shaders` generator | Dream → Slang with source maps, `slangc` + `dream-slang-layout` tool phase, Slang diagnostics mapped to Dream lines, reflection, DGPB bundle, typed `GpuShaders` wrappers. |
| Native host (`dream-host-gpu`) | slang-rhi C++ glue, strict backend selection, fence-based resource lifetime, async completions, window surfaces, ray-tracing pipelines and acceleration structures, `DREAM_GPU_STRICT_AOT=1`. |
| Browser host (`gpu.js`) | WebGPU over the WGSL bundle target, canvas surfaces, selective runtime inclusion. |

Verified on an Apple M3 with real Dream programs: compute (saxpy, workgroup reduction), offscreen render readback, subgroups, backend selection, forward- and reverse-mode autodiff against finite differences, and `float16` buffers with half arithmetic on Metal, Vulkan (MoltenVK) and WebGPU (Dawn); inline ray queries and 8x8 cooperative matrices on Metal.

Windows qualification on 2026-10-18 (RTX 4080, Windows driver 32.0.16.1714): the full native
GPU corpus passes. Vulkan and D3D12 execute full ray-tracing pipelines, inline ray queries and
bindless texture sampling. Vulkan also executes cooperative-vector MLP conversion/multiplication, cooperative matrices,
autodiff and half buffers. Native Vulkan, D3D12 and WebGPU pass compute, render readback,
real-window presentation, resize and device-loss checks. D3D12 resize required retiring completed
queue work before replacing the swapchain. Windows now really maps the Slang compiler DLL in
the strict-AOT self-test and verifies that device creation aborts.

Real browser WebGPU also passes compute/readback, offscreen pixel checks, canvas presentation,
resize and actual device destruction in Chromium and Edge on this machine. The reproducible
harness is `tooling/gpu-browser/` and the Linux software-browser job is in `ci.yml`.

CUDA compute is also verified on the RTX 4080 with NVRTC/NVCC 12.9.86 and CUDA runtime headers
12.9.79: the Dream saxpy/switch suite and the host's compute, entry-parameter, readback and error
checks pass without loading Slang. Driver calls now enter slang-rhi's `DeviceScope` on guest and
completion threads. CUDA resources and constant buffers bind by their uniform byte offsets;
the layout recorder stores the real entry-parameter list, rather than deriving it from a wrapper
layout. `dreamer toolchain install slang` rebuilds the recorder when its embedded sources change.

## Not yet verified

- **CI after these fixes.** The original PR head passes Linux/macOS and all sanitizer jobs. Windows passes build, clippy and workspace tests but fails a timed resource-cleanup assertion in its full corpus; that test now waits for cleanup instead of assuming a 10 ms timer. The local Windows full gate passes. The original benchmark gate reports regressions in `string_concat` and `substring_span`; those remain unresolved.

  A paired Windows comparison against the exact PR base (`19f20169`, 12 measured rounds,
  five passes per process) passes both rows: concatenation is 1.00x baseline and substring
  spans are 1.02x. Their optimized LLVM and assembly are identical between revisions on
  Windows and when cross-compiling for Linux, including the original runner's Zen 5 CPU.
  This does not establish Linux performance parity: executable layout and the runner still
  need checking. Benchmark CI now records the matching baseline runtime sources and retains
  optimized IR and optimization remarks alongside the timing samples.
- **D3D11** at run time: pinned slang-rhi has no D3D11 fence implementation, so device setup fails. CUDA compute and DXIL/DXBC/PTX output are now verified on Windows. CUDA cooperative vectors remain unavailable without OptiX in the pinned build.
- **Metal full ray-tracing pipelines** remain unavailable through Slang 2026.20 and MoltenVK. Vulkan and D3D12 pipelines now pass on the RTX 4080.
- **CUDA cooperative matrices/vectors** and D3D12 cooperative vectors. Vulkan cooperative vectors, optimal MLP layout conversion and 16x16 cooperative matrices now pass on the RTX 4080; Metal's 8x8 tiles were verified earlier.
- **Metal bindless sampling** remains unavailable in slang-rhi; MoltenVK lacks the required features. Vulkan and D3D12 sampling now pass on the RTX 4080.
- **Windows generator sandbox: verified.** The AppContainer/job-object tests now pass in CI and locally. The earlier `LOCALAPPDATA` environment-block fix resolves `ERROR_ENVVAR_NOT_FOUND` (203).

## Remaining milestones

### M7: machine learning features

The declarations and generator lowering exist (`@differentiable` → `[Differentiable]`, `GpuDiff.*` → `fwd_diff`/`bwd_diff`, `GpuCoopVec`/`GpuCoopMat`/`GpuMlpWeights` → `CoopVec`/`CoopMat`/`coopVecMatMulAdd`). Status of the items:

1. **Done.** `gpu_autodiff_forward.dream` (two-input scalar and `GpuVec2` tangents) and `gpu_autodiff_backward.dream` (the plan's `train_step`, driven for eight gradient-descent steps, plus `GpuVec2` and struct gradients) pass on Metal, Vulkan (MoltenVK) and WebGPU (Dawn), checked against double-precision central differences on the CPU.
2. **Done, verified on Vulkan hardware.** `gpuMlpUpload` (`crates/dream-host-gpu/src/mlp.rs`) converts the matrix to slang-rhi's inferencing-optimal layout on Vulkan and D3D12 devices with cooperative vectors and keeps it row-major elsewhere. The generated Slang reads it through `DREAM_MLP_LAYOUT`, which is optimal on `spirv`/`dxil` and row-major elsewhere, and the layer buffer holds biases at 0 and the matrix at the next 64-byte boundary. Layout selection and packing are unit-tested. CUDA stays row-major: the PTX target is compiled without OptiX, so Slang emulates cooperative vectors there, and the pinned slang-rhi build leaves out OptiX and NVAPI, so CUDA and D3D12 devices never report the feature. The RTX 4080 Vulkan qualification now executes both the optimal layout conversion and native multiply against the CPU reference.
3. **Done.** `gpu_cooperative.dream` evaluates a two-layer `float16` MLP (Vulkan) and multiplies a tile (16x16 on Vulkan, 8x8 on Metal) against CPU references. It skips when the feature is missing, after checking that such a device reports `UnsupportedCapability`. Cooperative matrices now also target Metal: Slang lowers 8x8 `CoopMat` to `simdgroup_matrix`, and Apple-silicon Metal devices report `CooperativeMatrices`. The generator rejects other tile sizes on Metal, and rejects cooperative vectors on Metal/WebGPU-only shaders and cooperative matrices on WebGPU-only shaders at their first use, before Slang's E36107 (goldens in `tests/gpu_shaders.rs`).
4. **Done.** `gpu_float16_buffers.dream` uploads and reads back `GpuBuffer<float16>`, runs half arithmetic with a `float16` value parameter, widens to `float`, and runs `sqrt` on Metal, Vulkan and WebGPU (Dawn's `shader-f16`), with an odd element count. Shaders skip without `GpuFeature.Float16`. `gpu.js` requests `shader-f16` and gates `half` entries on it (`scripts/test-gpu-host.mjs`). `float16` cannot cross `extern`/`@export`, so buffers travel as binary16 bytes, which are 2 bytes on wasm32 as well.
5. **Decided: sizes stay on `@coop_shape`; no const generics.** The sizes matter only inside shaders, where the generator already knows them: from `@coop_shape` on parameters and fields, from the constant passed to `GpuCoopVec.zeros` or `GpuCoopMat.zeros`/`load`, and from the layer that `apply` uses. On the CPU every size is a runtime value. `upload_mlp` takes `inputs`/`outputs`, and a `GpuBuffer<GpuCoopVec<T>>` parameter is a flat `GpuBuffer<T>` in the generated wrapper, so const generics would add nothing that CPU code can use. The one check they would have given, that an uploaded layer matches the shader, is now made by the generated wrapper: `GpuArgs.mlp_weights` receives the parameter's `@coop_shape`, and a layer of other sizes fails with `GpuError.InvalidResource` before the pipeline is created. Value generics would need a new kind of generic parameter through the parser, sema, `dream-types` interning and monomorphization keys (which stay `(DefId, type args)` per [the stack-allocation and mono note](10-stack-alloc-and-mono-design-note.md)), all for generator-only types. Revisit only if a CPU-side API needs a size in a type, such as fixed-length vectors outside shaders.

### Test coverage on other platforms

1. Windows CI runner: D3D12, D3D11 and Vulkan suites; Windows sandbox tests; `x86_64-pc-windows-msvc` host build.
2. **Confirmed.** Linux lavapipe executes ray-tracing pipelines and bindless sampling in debug and release corpus runs; cooperative vectors/matrices skip because the features are absent. `ci.yml` now runs window and GPU presentation tests under Xvfb; that new display job still needs CI qualification.
3. **CUDA compute manual run done.** `python scripts/qualify-gpu.py --backend cuda` requires actual execution, failing if the test only skips. CUDA cooperative vectors remain blocked by the pinned build's lack of OptiX.
4. **Vulkan and D3D12 manual runs done** on the Windows RTX 4080. A recurring hardware-RT runner remains.
5. **Done.** `tooling/gpu-browser/` checks real browser compute, render readback, presentation, resize and device loss. Chromium and Edge pass on the Windows RTX 4080; Linux CI uses SwiftShader.
6. **Windows manual run done** for Vulkan, D3D12 and WebGPU with `DREAM_GPU_SURFACE_TEST=1`; display-backed CI coverage remains.

### Docs and final gate

1. **Done.** The guide is `docs/library/gpu/` (setup and compute, shaders and the generated `GpuShaders` class, rendering with `system.window`, ray tracing, machine learning). Full ray-query and autodiff programs live in those pages; the cookbook adds a histogram (atomics) and a mouse-driven fragment shader. Every full program is compile-checked by `tooling/docs/check_examples.py`.
2. **Done.** The inventory is refreshed and the docs gate passes.
3. **Done.** `make_bundles.py --check` runs in the Linux and macOS build jobs. Windows stages the SDK's `dxcompiler.dll` and `dxil.dll` inside the fingerprinted Slang root, then regenerates the fixtures with DXIL/DXBC before the host matrix. Fixture reflection compiles each entry separately, as required by DXBC. The triangle uses the explicit Vulkan vertex index except on D3D, where that semantic is invalid.
4. **Done.** X11 packages and the window capability library are in `release.yml` and `phase34.yml`.
5. **Local Windows gate passes:** workspace build, strict clippy, workspace tests, and the full native/Node corpus. Shader goldens and per-host byte determinism also pass; SPIR-V golden comparisons preserve sizes/layouts/bindings while allowing Slang's host-dependent blob hashes.

## Known limitations and follow-up bugs

### Language and compiler

- `@generator_only` cannot mark globals (globals take no attributes), so built-ins are functions: `global_id().x`.
- No attributes on locals, so workgroup memory is `workgroup_array<T>(n)`.
- No generic overloads (`GpuDiff.backward2`/`backward3`) and no default array parameters (`ray_tracing_pipeline` takes `callables: []`).
- Unions cannot implement `Error`, have static methods or override `to_string`; `GpuError` offers `code()`/`message()` and `?` across `WindowError`/`GpuError` needs `map_err`.
- `sizeof(T)` inside generic class methods reports "unknown type 'unknown'" (worked around with `Bytes.of(Buffer.zero<T>()).length`).
- Top-level variable names are program-wide; the generator prefixes its globals to avoid collisions.
- `dream run` prints nothing when the guest segfaults (exit 139).
- Possible remaining stale-cache case: a float local written in a loop over `switch` on `Option<SomeClass>` with a `float[]` field reported a missing `array_to_string_t*` runtime symbol. A related `int[]` field bug was fixed in `reach.rs`; re-check this repro.
- `system.gpu` imports `system.codegen`, which makes every GPU program compile a larger stdlib.

### Generator

- Shaders whose `@gpu_targets` fit none of the build's targets (a Metal-only shader in a Linux build) are left out with a warning; the module still gets its wrapper, and running the shader returns `GpuError.BackendNotCompiled`. The hosts cannot tell such an entry from an unknown name, which only raw host calls could produce.
- Shader functions cannot be named after Slang keywords (`unused`, `double`, `layout`, ...), because entry points keep their Dream name; helpers and locals are renamed instead.
- C-style `switch` statements now work in shaders, including multiple labels and a default. Check-only analysis previously lost label HIR and rejected constant labels; a scoped expression capture fixes that. Pattern switch expressions remain unsupported.
- Outside a shader, `GpuMath` and the other shader-only types do not exist, and the analyzer reports "variable GpuMath does not exist" rather than pointing at the shader requirement.
- Unsupported in shaders: user struct methods, operator overloads, explicit enum values, named arguments, switch expressions, `for` loops over non-ranges.
- Structs passed by value or stored in buffers use Slang's GPU layout (`GpuVec3` padding differs from the CPU layout).
- Cooperative types: cooperative matrices are 8x8 only on Metal and unavailable on D3D12 (Slang lowers `CoopMat` only for SPIR-V, Metal and CUDA); cooperative vectors have no Metal or WebGPU lowering. Shaders that use them cannot also target D3D12 or CUDA in a portable test, because those targets need `dxc` and NVRTC on the build machine.
- Ray-tracing stages cannot target Metal; non-raygen stages cannot take resource parameters; `workgroup_count()` is rejected on Metal and WebGPU.
- Target errors point at the module or function declaration because attribute nodes have no ids.
- **Done.** Expression emission lives in `slang_expr.dream`; statement emission and shared body state remain in `slang_body.dream`.

### Native host

- Vertex and fragment shaders must come from the same module; vertex locations must be contiguous from 0.
- Ray-tracing payload, hit-attribute size and recursion depth are fixed at 128 bytes, 32 bytes and 1.
- CUDA needs the toolkit at run time: pinned slang-rhi uses NVRTC to compile its own texture-clear support kernels during device creation. Dream shader code remains precompiled PTX and no Slang compiler is loaded.
- D3D buffer binding distinguishes read-only SRVs from writable UAVs even when both use register 0. Text shader blobs normalize Slang's Windows CRLF output to LF. DXIL shader models are selected per pipeline group from its required capabilities.
- Slang 2026.20 emits invalid MSL for a module with both a `GpuHandle` buffer and ray queries.
- Native Wayland windows are unsupported (XWayland only).
- Vulkan optimal MLP conversion and native cooperative-vector multiplication pass on the RTX 4080. CUDA and D3D12 still never report cooperative vectors because the pinned slang-rhi build leaves out OptiX and NVAPI.
- Layouts are recorded per pipeline group, so a compute pipeline binds only its shader's resources (`gpu_pipeline_layouts.dream` runs three shaders with 12 storage buffers together on Dawn). A module's render shaders share one layout, as do its ray-tracing shaders, so on WebGPU their storage buffers together count toward the per-stage limit (8 by default, 10 on Dawn over Metal); the generator warns past 8. Per-pair render layouts would need the vertex/fragment pairing at build time.
- On WebGPU, buffer writes that start or end inside a 4-byte word fail unless they run to the end of the buffer; allocations are padded to whole words on every backend.

### Browser host

- Texture sample types and sampler kinds default to float/filtering when the WGSL declaration does not say otherwise.
- Buffer writes and copies must be 4-byte aligned.
- Bundle hash failures and synchronous validation errors surface at the next submit.
- The vsync hint is ignored; `gpu.js` is 539 lines.

### Window

- Native gamepad rumble returns `false` (GLFW has no force feedback).
- Browser frame loops must `await` each frame to receive input; gamepad edges are sampled per poll.

### Framework

- LSP references and completion inside consumed bodies have no dedicated tests.
- Pinned tool archives are verified with SHA-256 (what upstream publishes), BLAKE3 only for fingerprints.
