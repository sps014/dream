# GpuMath

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

This type spans several pages: [part 1](gpu-gpu-math-1.md) · [part 2](gpu-gpu-math-2.md).

## `saturate`

`x` limited to the range `0` through `1`.

```dream
public static extern fun saturate<T>(x: T): T
```

## `mix`

Linear interpolation: `a` when `t` is `0`, `b` when `t` is `1`.

```dream
public static extern fun mix<T>(a: T, b: T, t: T): T
```

## `step`

`0` where `x` is below `edge`, otherwise `1`.

```dream
public static extern fun step<T>(edge: T, x: T): T
```

## `smoothstep`

A smooth curve from `0` at `low` to `1` at `high`.

```dream
public static extern fun smoothstep<T>(low: T, high: T, x: T): T
```

## `fma`

`a * b + c`, computed with a single rounding where the GPU supports it.

```dream
public static extern fun fma<T>(a: T, b: T, c: T): T
```

## `reflect`

The direction of `incident` reflected about the unit-length surface `normal`.

```dream
public static extern fun reflect<T>(incident: T, normal: T): T
```

## `refract`

The direction of unit-length `incident` refracted through a surface with unit-length `normal`, where `eta` is the ratio of refractive indices; zero on total internal reflection.

```dream
public static extern fun refract<T>(incident: T, normal: T, eta: float): T
```

## `ddx`

How fast `x` changes between horizontally neighboring pixels. Fragment shaders only.

```dream
public static extern fun ddx<T>(x: T): T
```

## `ddy`

How fast `x` changes between vertically neighboring pixels. Fragment shaders only.

```dream
public static extern fun ddy<T>(x: T): T
```

## `fwidth`

`abs(ddx(x)) + abs(ddy(x))`, for antialiasing procedural edges. Fragment shaders only.

```dream
public static extern fun fwidth<T>(x: T): T
```
