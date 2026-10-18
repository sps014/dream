# GPU shader bundle

The `system.gpu.shaders` generator compiles every shader module ahead of time and embeds one bundle per module as an asset. `GpuShaderLibrary.register` hands the bundle bytes to the host through `gpuLibraryLoad`. Native hosts (slang-rhi) and the browser host (`runtime/src/hosts/gpu.js`) read the same format; no host ever compiles Slang source.

The format grew out of the M0 probe in `crates/dream-host-gpu/aot_probe/` and replaces the probe's reader. Dream is pre-1.0, so the version stays `0` and changes in place.

## Producer pipeline

For each shader module and each selected target the generator runs, in one tool phase:

1. `slangc <module>.slang <options> -target <target> -entry <name> -stage <stage> -o <code> -reflection-json <reflection.json>`, once per entry point. (A single `slangc` call with several entry points writes one combined module rather than per-entry code: its SPIR-V entry points keep their own names instead of `main`, and its reflection reports entry-point bindings relative to the entry.)
2. `dream-slang-layout <module>.slang <options> -target <target> -entry <name> -stage <stage> ... -o <layout.bin>`, once per module and target with every entry point in declaration order. This is the M0 layout recorder, installed next to `slangc` in the pinned `slang` tool root by `dreamer toolchain install slang`. It answers every reflection query slang-rhi makes and writes them to the layout blob that the native replay library serves at run time. The last `-o` names the layout; `-entry`/`-stage` and `-reflection-json` are accepted as slangc spells them (see [Native GPU host](19-gpu-host.md)).

Shader parameters are module-scope Slang globals: `<entry>_<parameter>` for resources and one `ConstantBuffer` named `<entry>_params` for an entry's value parameters. Slang lays out module globals the same way whichever entry point is compiled, so each per-entry code blob agrees with the module's recorded layout. Native builds record a layout for every target, `wgsl` included (Dawn); while `dream-slang-layout` is not installed, native bundles carry an empty layout, which native hosts reject.

Every run defines `DREAM_TARGET_<NAME>` for its target (`DREAM_TARGET_SPIRV`, `DREAM_TARGET_METAL`, …, from the target name in upper case; the `metallib` run defines `DREAM_TARGET_METAL`). An entry whose `@gpu_targets` leave out some of the module's targets is wrapped, together with its parameter globals, in `#if defined(DREAM_TARGET_A) || …` for the targets it has. Slang's module loader, which `dream-slang-layout` uses, checks every entry point of a module against the compilation target, so an unguarded ray tracing entry would stop the WGSL layout of the module's other shaders.

Code and layout outputs are ordinary tool outputs that the generator reads, hashes and packs. Fixed options for every target: `-matrix-layout-row-major` and the `DREAM_TARGET_<NAME>` define; never `-fvk-use-entrypoint-name` (slang-rhi names the SPIR-V entry point `main`).

Target names and the backends that consume them:

| Target name | Code format | Backend |
|---|---|---|
| `dxil` | `dxil` | `Dx12` |
| `dxbc` | `dxbc` | `Dx11` |
| `spirv` | `spirv` | `Vulkan` |
| `metal` | `metallib`, or `msl` source when the build host has no Apple `metal` tool | `Metal` |
| `ptx` | `ptx` | `Cuda` (compute only) |
| `wgsl` | `wgsl` | `WebGpu` (Dawn natively, the browser on wasm32) |

The generator includes only the targets that the module's `@gpu_targets` selects and that the build triple can load: wasm32 builds carry `wgsl` only, and the layout blob is omitted (size 0) because the browser binds from the parameter table. Native builds keep `wgsl` (Dawn) plus `metal` and `spirv` on Apple triples (Vulkan runs there through the Khronos loader and MoltenVK), `spirv` and `ptx` on Linux, and every target but `metal` on Windows. For `metal` it requests both `-target metallib` and `-target metal` and uses the metallib only when every entry compiled to one.

Each Dream module with shaders gets one bundle, embedded as the asset `gpu_shaders.<stem>`, where `<stem>` is the module path with `.` replaced by `_` (`main` for the unnamed root module). `GpuShaderLibrary.register` receives the module path as the library name.

## Binary layout

All integers are little-endian `u32`. A string is a `u32` byte length followed by UTF-8 bytes. A blob reference is `offset u32, size u32, sha256[32]`; offsets are relative to the start of the data section, which begins at `header_size`. Identical blobs are stored once and share one reference.

```text
"DGPB"  version=0  header_size  target_count
target × target_count:
    name: string                 // table above, sorted by name
    code_format: string
    layout: blob
    entry_count
    entry × entry_count:         // in Dream declaration order
        name: string             // the Dream function name
        stage: string            // compute | vertex | fragment | raygeneration | closesthit | anyhit | miss | intersection | callable
        code: blob
        thread_group_size: u32 × 3   // 0,0,0 for non-compute stages
        param_count
        param × param_count:     // bindable parameters, in Dream parameter order
            name: string
            kind                 // GpuArgKind numbering (crates/dream-stdlib/src/system/gpu/gpu_arg_kind.dream)
            flags                // bit 0: read-only (`@readonly`), bit 1: written by the shader
            space
            binding
            offset               // byte offset inside the uniform block for value parameters, else 0
            size                 // byte size for value parameters, else 0
        capability_count
        capability: string × capability_count   // features this entry needs, sorted (table below)
data section
```

Only bindable parameters (resources and values) are listed; stage inputs (vertex attributes, interpolated fragment inputs, ray payloads, hit attributes and callable data) come from the pipeline. A row's index is therefore the parameter's position among the bindable ones, which is what `GpuArgs` and `GpuBindingSlot` carry. `name` is the Dream parameter name.

`space`/`binding` come from the target's `-reflection-json`: the descriptor set and binding on SPIR-V/WGSL, the register space and register on DXIL/DXBC, and the `[[buffer]]`, `[[texture]]` or `[[sampler]]` index on Metal (unique only together with `kind`). Value parameters report the space and binding of the entry's uniform block. A `GpuMlpWeights` parameter is a byte-address buffer holding the `outputs` biases at offset 0 and the `outputs × inputs` matrix at the next multiple of 64 bytes, both of its element type. The generated Slang reads the matrix through `DREAM_MLP_LAYOUT`, defined per target: the inferencing-optimal layout on `spirv` and `dxil`, where Slang multiplies natively, and row-major elsewhere (`ptx` gets Slang's per-element emulation). `gpuMlpUpload` stores the matrix to match (see [Native GPU host](19-gpu-host.md)).

Capability names, one per feature an entry point uses (from the symbols its body and helpers reach). They are listed per entry, so one shader's requirement never blocks the module's other shaders:

| Name | Feature |
|---|---|
| `raytracing` | ray tracing stages, `trace_ray`, `call_shader`, `report_hit` |
| `rayquery` | inline ray queries (`GpuRayQuery`) |
| `descriptor_handle` | bindless resources (`GpuHandle<T>`) |
| `subgroup_basic` | subgroup operations |
| `cooperative_vector` | `GpuCoopVec`, `GpuMlpWeights` |
| `cooperative_matrix` | `GpuCoopMat` |
| `half` | `float16` values |
| `atomic64` | atomics on 64-bit buffer elements |

## Consumers

- A `Bind` command in `gpuSubmit` carries the parameter's index into `param × param_count`; hosts never look parameters up by name at bind time.
- Creating a pipeline checks each of its entries' capabilities against the device and fails with `UnsupportedCapability(feature)` for the first one it lacks; it never changes backend. An unknown capability name makes the bundle invalid.
- Native hosts pick the target for the device's backend, call the replay library's `createPrecompiledProgram(layout, code blobs)`, and verify each blob's SHA-256 before use. A bundle with no target for the device's backend makes `GpuDevice.create` fail with `BackendNotCompiled`.
- The browser host reads the `wgsl` target, creates `GPUShaderModule`s from the code and bind group layouts from the parameter table. One WGSL blob can hold several entry points, so the host selects each by `name`: the generator must keep the Dream function name as the WGSL entry point's function name. What the table does not carry (buffer access mode, texture sample type, storage texture format) comes from the WGSL declaration at the parameter's `@group`/`@binding`.

The generator also emits the readable `<stem>.bundle.json` view next to the `<stem>.slang` artifact for review and goldens; hosts never read it.
