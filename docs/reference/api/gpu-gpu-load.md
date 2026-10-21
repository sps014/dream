# GpuLoad

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `GpuLoad`

What a render pass does with a target's existing contents when it starts.  For color targets `T` is `GpuVec4` and for depth targets `float`, so `GpuLoad.Clear(GpuVec4(0.0f, 0.0f, 0.0f, 1.0f))` clears to opaque black and `GpuLoad.Clear(1.0f)` clears depth to the far plane.

```dream
public enum GpuLoad<T>
```

## `Clear`

Fills the target with `value` first.

```dream
Clear(value: T)
```

## `Keep`

Keeps what the target already holds, to draw on top of earlier passes.

```dream
Keep
```
