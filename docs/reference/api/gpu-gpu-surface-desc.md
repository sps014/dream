# GpuSurfaceDesc

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct GpuSurfaceDesc`

Size and presentation options for `GpuSurface.configure`.

```dream
public struct GpuSurfaceDesc
```

## `width: int`

```dream
public width: int
```

## `height: int`

```dream
public height: int
```

## `present_mode: GpuPresentMode`

```dream
public present_mode: GpuPresentMode
```

## `alpha_mode: GpuAlphaMode`

```dream
public alpha_mode: GpuAlphaMode
```

## `color_space: GpuColorSpace`

```dream
public color_space: GpuColorSpace
```

## `max_pixel_ratio: float`

Cap on backing-store / drawable scale versus CSS (web) or logical (native) size. `1` keeps today's 1:1 swapchain (cheap on phones). `2` is a typical game clamp: drawable = client × min(devicePixelRatio, 2). Values `<= 1` mean 1:1.

```dream
public max_pixel_ratio: float
```

## `of`

`width`×`height` with vsync (`Fifo`), opaque compositing, sRGB, 1:1 pixels.

```dream
public static fun of(width: int, height: int): GpuSurfaceDesc
```
