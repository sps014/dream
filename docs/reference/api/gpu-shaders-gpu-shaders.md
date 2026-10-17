# gpu_shaders

**Import:** `import system.gpu.shaders;`

The generator behind `import system.gpu`: programs never import this package themselves. Read [GPU shader bundle](../../internals/18-gpu-shader-bundle.md) for the bundle it embeds. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `gpu_shaders`

Compiles the program's GPU shaders ahead of time. Phase 0 checks every shader against the shader rules, translates each module's shaders to a readable Slang file (published next to the build output) and asks the pinned `slangc` for every `@gpu_targets` target. Phase 1 maps Slang errors back to Dream code, embeds one shader bundle per module and emits the module's `GpuShaders` class.

```dream
public fun gpu_shaders(ctx: GenContext): void
```
