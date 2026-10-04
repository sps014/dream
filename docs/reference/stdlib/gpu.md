# `system.gpu`

WebGPU compute and draw from Dream. Auto-imported when you write `@compute`, `@vertex`, or `@fragment`. You can also `import system.gpu;`.

Language: [Compute shaders](../language/compute.md), [Vertex & fragment](../language/shaders.md).

Cookbook: [GPU SAXPY](../../cookbook/gpu-saxpy.md), [GPU triangle](../../cookbook/gpu-triangle.md).

```dream
import system;
import system.gpu;

async fun main(): void {
    if (Gpu.try_init().await).is_err() {
        System.println("gpu unavailable");
        return;
    }
}
```

## Explore this topic

- [Set up a GPU device](gpu-device.md)
- [GPU buffers](gpu-buffers.md)
- [Run GPU calculations](gpu-compute.md)
- [Textures and rendering](gpu-rendering.md)
- [Vectors and matrices](gpu-math.md)

## Input and complete API lookup

Use [Surfaces and input](gpu-input.md) for window, canvas, pointer, keyboard, and gamepad state. The [GPU API catalog](../api/system-gpu.md) lists all public resource methods, settings, and enum choices.

GPU operations need an available device in the selected environment. Portable CPU math helpers do not require the device. Check errors and capabilities rather than assuming another machine supports the same features.
