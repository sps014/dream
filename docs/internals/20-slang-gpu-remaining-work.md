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

## Not yet verified

- **Full gate on the combined tree.** Each workstream ran focused tests only. `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -- --ignored` and the full corpus (`python3 scripts/test_corpus.py --both`) have not run on this branch. `cargo check --workspace --all-targets` passes.
- **Windows and Linux builds** of `dream-host-gpu` and `dream-host-window`. The `gpu` and `window` features are default-on, so CI is their first build.
- **D3D12, D3D11 and CUDA** at run time, and DXIL/DXBC/PTX output (needs `dxc`, `fxc`, NVRTC reachable from the tool sandbox).
- **Ray-tracing pipelines** on hardware (no RT on Metal through Slang 2026.20 or on MoltenVK). Linux CI installs lavapipe and may exercise Vulkan paths; unconfirmed.
- **Cooperative vectors** on any device, including the inferencing-optimal MLP layout conversion, and **cooperative matrices** on Vulkan or CUDA (only Metal's 8x8 tiles have run).
- **Bindless** on any backend (slang-rhi has no Metal descriptor handles; MoltenVK lacks required features).
- **Real browser WebGPU.** `gpu.js` is tested against a fake `navigator.gpu` and the real generator bundles only.
- **Windows generator sandbox** (job object + AppContainer) has never executed.

## Remaining milestones

### M7: machine learning features

The declarations and generator lowering exist (`@differentiable` → `[Differentiable]`, `GpuDiff.*` → `fwd_diff`/`bwd_diff`, `GpuCoopVec`/`GpuCoopMat`/`GpuMlpWeights` → `CoopVec`/`CoopMat`/`coopVecMatMulAdd`). Status of the items:

1. **Done.** `gpu_autodiff_forward.dream` (two-input scalar and `GpuVec2` tangents) and `gpu_autodiff_backward.dream` (the plan's `train_step`, driven for eight gradient-descent steps, plus `GpuVec2` and struct gradients) pass on Metal, Vulkan (MoltenVK) and WebGPU (Dawn), checked against double-precision central differences on the CPU. The suite is two modules because of the WebGPU storage-buffer limit under "Native host" below.
2. **Done, unverified on hardware.** `gpuMlpUpload` (`crates/dream-host-gpu/src/mlp.rs`) converts the matrix to slang-rhi's inferencing-optimal layout on Vulkan and D3D12 devices with cooperative vectors and keeps it row-major elsewhere. The generated Slang reads it through `DREAM_MLP_LAYOUT`, which is optimal on `spirv`/`dxil` and row-major elsewhere, and the layer buffer holds biases at 0 and the matrix at the next 64-byte boundary. Layout selection and packing are unit-tested. CUDA stays row-major: the PTX target is compiled without OptiX, so Slang emulates cooperative vectors there, and the pinned slang-rhi build leaves out OptiX and NVAPI, so CUDA and D3D12 devices never report the feature. No available machine has a cooperative-vector device, so the conversion and the native multiply have never run.
3. **Done.** `gpu_cooperative.dream` evaluates a two-layer `float16` MLP (Vulkan) and multiplies a tile (16x16 on Vulkan, 8x8 on Metal) against CPU references. It skips when the feature is missing, after checking that such a device reports `UnsupportedCapability`. Cooperative matrices now also target Metal: Slang lowers 8x8 `CoopMat` to `simdgroup_matrix`, and Apple-silicon Metal devices report `CooperativeMatrices`. The generator rejects other tile sizes on Metal, and rejects cooperative vectors on Metal/WebGPU-only shaders and cooperative matrices on WebGPU-only shaders at their first use, before Slang's E36107 (goldens in `tests/gpu_shaders.rs`).
4. **Done.** `gpu_float16_buffers.dream` uploads and reads back `GpuBuffer<float16>`, runs half arithmetic with a `float16` value parameter, widens to `float`, and runs `sqrt` on Metal, Vulkan and WebGPU (Dawn's `shader-f16`), with an odd element count. Shaders skip without `GpuFeature.Float16`. `gpu.js` requests `shader-f16` and gates `half` entries on it (`scripts/test-gpu-host.mjs`). `float16` cannot cross `extern`/`@export`, so buffers travel as binary16 bytes, which are 2 bytes on wasm32 as well.
5. **Decided: sizes stay on `@coop_shape`; no const generics.** The sizes matter only inside shaders, where the generator already knows them: from `@coop_shape` on parameters and fields, from the constant passed to `GpuCoopVec.zeros` or `GpuCoopMat.zeros`/`load`, and from the layer that `apply` uses. On the CPU every size is a runtime value. `upload_mlp` takes `inputs`/`outputs`, and a `GpuBuffer<GpuCoopVec<T>>` parameter is a flat `GpuBuffer<T>` in the generated wrapper, so const generics would add nothing that CPU code can use. The one check they would have given, that an uploaded layer matches the shader, is now made by the generated wrapper: `GpuArgs.mlp_weights` receives the parameter's `@coop_shape`, and a layer of other sizes fails with `GpuError.InvalidResource` before the pipeline is created. Value generics would need a new kind of generic parameter through the parser, sema, `dream-types` interning and monomorphization keys (which stay `(DefId, type args)` per [the stack-allocation and mono note](10-stack-alloc-and-mono-design-note.md)), all for generator-only types. Revisit only if a CPU-side API needs a size in a type, such as fixed-length vectors outside shaders.

### Test coverage on other platforms

1. Windows CI runner: D3D12, D3D11 and Vulkan suites; Windows sandbox tests; `x86_64-pc-windows-msvc` host build.
2. Linux CI: Vulkan on lavapipe (confirm which suites execute rather than skip); X11 window tests.
3. A CUDA runner (or a documented manual run) for compute and cooperative vectors.
4. A hardware-RT runner (Vulkan or D3D12) for `gpu_ray_tracing_pipeline.dream`.
5. Headless Chrome with WebGPU for `gpu.js` presentation, resize and device loss.
6. Presentation and device-loss tests on real windows (`DREAM_GPU_SURFACE_TEST=1`, `DREAM_WINDOW_DISPLAY_TEST=1`) in CI where a display is available.

### Docs and final gate

1. Learn guide, cookbook (compute, render loop with `system.window`, ray query, autodiff) and reference pages for the generated `GpuShaders` wrapper shape.
2. Refresh `tooling/docs/api-coverage.json` after reviewing reference pages; run the docs gate from `AGENTS.md`.
3. Bundle freshness: a CI check that the committed `crates/dream-host-gpu/tests/fixtures/*.dgpb` match `make_bundles.py`.
4. CI workflow gaps: X11 dev packages in `release.yml` and `phase34.yml`; `release.yml` does not ship the window capability library.
5. The full pre-commit gate from `AGENTS.md` on native and Node.

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

- Unsupported in shaders: user struct methods, operator overloads, explicit enum values, named arguments, switch expressions, `for` loops over non-ranges.
- Structs passed by value or stored in buffers use Slang's GPU layout (`GpuVec3` padding differs from the CPU layout).
- Cooperative types: cooperative matrices are 8x8 only on Metal and unavailable on D3D12 (Slang lowers `CoopMat` only for SPIR-V, Metal and CUDA); cooperative vectors have no Metal or WebGPU lowering. Shaders that use them cannot also target D3D12 or CUDA in a portable test, because those targets need `dxc` and NVRTC on the build machine.
- Ray-tracing stages cannot target Metal; non-raygen stages cannot take resource parameters; `workgroup_count()` is rejected on Metal and WebGPU.
- Target errors point at the module or function declaration because attribute nodes have no ids.
- `slang_body.dream` is ~650 lines; split it.

### Native host

- Vertex and fragment shaders must come from the same module; vertex locations must be contiguous from 0.
- Ray-tracing payload, hit-attribute size and recursion depth are fixed at 128 bytes, 32 bytes and 1.
- Slang 2026.20 emits invalid MSL for a module with both a `GpuHandle` buffer and ray queries.
- Native Wayland windows are unsupported (XWayland only).
- The inferencing-optimal MLP conversion and native cooperative-vector multiply have not run on hardware (no Vulkan/D3D12 device with cooperative vectors available); CUDA and D3D12 never report cooperative vectors because the pinned slang-rhi build leaves out OptiX and NVAPI.
- WebGPU (Dawn) pipelines bind the globals of every shader in their module: the bundle records one layout per module and target. A module whose shaders together use more storage buffers than the per-stage limit (8 by default, 10 on Dawn over Metal) fails at dispatch. The generator warns past 8; the fix is per-pipeline layouts in the bundle plus a filtered replay layout.
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
