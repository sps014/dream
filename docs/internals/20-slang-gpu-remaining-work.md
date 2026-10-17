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

Verified on an Apple M3 with real Dream programs: compute (saxpy, workgroup reduction), offscreen render readback, subgroups, and backend selection on Metal, Vulkan (MoltenVK) and WebGPU (Dawn); inline ray queries on Metal.

## Not yet verified

- **Full gate on the combined tree.** Each workstream ran focused tests only. `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -- --ignored` and the full corpus (`python3 scripts/test_corpus.py --both`) have not run on this branch. `cargo check --workspace --all-targets` passes.
- **Windows and Linux builds** of `dream-host-gpu` and `dream-host-window`. The `gpu` and `window` features are default-on, so CI is their first build.
- **D3D12, D3D11 and CUDA** at run time, and DXIL/DXBC/PTX output (needs `dxc`, `fxc`, NVRTC reachable from the tool sandbox).
- **Ray-tracing pipelines** on hardware (no RT on Metal through Slang 2026.20 or on MoltenVK). Linux CI installs lavapipe and may exercise Vulkan paths; unconfirmed.
- **Bindless** on any backend (slang-rhi has no Metal descriptor handles; MoltenVK lacks required features).
- **Real browser WebGPU.** `gpu.js` is tested against a fake `navigator.gpu` and the real generator bundles only.
- **Windows generator sandbox** (job object + AppContainer) has never executed.

## Remaining milestones

### M7: machine learning features

The declarations and generator lowering exist (`@differentiable` → `[Differentiable]`, `GpuDiff.*` → `fwd_diff`/`bwd_diff`, `GpuCoopVec`/`GpuCoopMat`/`GpuMlpWeights` → `CoopVec`/`CoopMat`/`coopVecMatMul`). Left:

1. Runtime suites in `tests/projects/gpu_runtime/` for forward and backward autodiff (`train_step` from the plan) on Metal, Vulkan and WebGPU, checked against CPU finite differences.
2. `gpuMlpUpload`: replace the row-major store with slang-rhi's device-specific matrix layout conversion on Vulkan (NV cooperative vector), D3D12 and CUDA/OptiX; keep row-major for the emulated path.
3. Cooperative vector/matrix suites that skip when `GpuFeature.CooperativeVectors`/`CooperativeMatrices` is missing; generator errors at first use when only Metal/WGSL targets are selected (Slang E36107) stay covered by goldens.
4. `float16` GPU buffers: `GpuBuffer<float16>` upload/readback, `shader-f16` on WebGPU, half arithmetic in shaders, and the `Float16` feature gate.
5. Decide whether `GpuCoopVec<T>`/`GpuMlpWeights<T>` sizes stay on `@coop_shape` or need const generics in the language.

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
- Ray-tracing stages cannot target Metal; non-raygen stages cannot take resource parameters; `workgroup_count()` is rejected on Metal and WebGPU.
- Target errors point at the module or function declaration because attribute nodes have no ids.
- `slang_body.dream` is ~650 lines; split it.

### Native host

- Vertex and fragment shaders must come from the same module; vertex locations must be contiguous from 0.
- Ray-tracing payload, hit-attribute size and recursion depth are fixed at 128 bytes, 32 bytes and 1.
- Slang 2026.20 emits invalid MSL for a module with both a `GpuHandle` buffer and ray queries.
- Native Wayland windows are unsupported (XWayland only).
- `gpuMlpUpload` stores weights row-major without device repacking (M7 item 2).

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
