# GpuMath

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-math.md)

## `normalize`

Unit `GpuVec3`; near-zero input yields `(0, 1, 0)` on CPU (WGSL `normalize`). Overloaded for `GpuVec2` / `GpuVec4`.

```dream
public static fun normalize(v: GpuVec3): GpuVec3
```

## `dot`

Dot product of two `GpuVec3`s (WGSL `dot`). Overloaded for `GpuVec2` / `GpuVec4`.

```dream
public static fun dot(a: GpuVec3, b: GpuVec3): float
```

## `cross`

Cross product of two `GpuVec3`s (WGSL `cross`).

```dream
public static fun cross(a: GpuVec3, b: GpuVec3): GpuVec3
```

## `reflect`

Reflects incident `i` about unit normal `n` (WGSL `reflect`).

```dream
public static fun reflect(i: GpuVec3, n: GpuVec3): GpuVec3
```

## `mix`

Linear blend `a * (1 - t) + b * t` (WGSL `mix`).

```dream
public static fun mix(a: float, b: float, t: float): float
```

## `pow`

Power `x^e` (WGSL `pow`).

```dream
public static fun pow(x: float, e: float): float
```

## `exp`

Exponential `e^x` via `pow` on CPU (WGSL `exp`).

```dream
public static fun exp(x: float): float
```

## `mul`

Matrix × vector (`mat2 * vec2` in WGSL). Overloaded for mat3/mat4 and mat×mat.

```dream
public static fun mul(m: GpuMat2, v: GpuVec2): GpuVec2
```

## `mul`

Matrix × vector (`mat3 * vec3` in WGSL).

```dream
public static fun mul(m: GpuMat3, v: GpuVec3): GpuVec3
```

## `mul`

Matrix × vector (`mat4 * vec4` in WGSL).

```dream
public static fun mul(m: GpuMat4, v: GpuVec4): GpuVec4
```

## `mul`

Matrix × matrix (`mat2 * mat2` in WGSL).

```dream
public static fun mul(a: GpuMat2, b: GpuMat2): GpuMat2
```

## `mul`

Matrix × matrix (`mat3 * mat3` in WGSL).

```dream
public static fun mul(a: GpuMat3, b: GpuMat3): GpuMat3
```

## `mul`

Matrix × matrix (`mat4 * mat4` in WGSL).

```dream
public static fun mul(a: GpuMat4, b: GpuMat4): GpuMat4
```

## `transpose`

Transpose of a `GpuMat2` (WGSL `transpose`). Overloaded for mat3/mat4.

```dream
public static fun transpose(m: GpuMat2): GpuMat2
```

## `transpose`

Transpose of a `GpuMat3` (WGSL `transpose`).

```dream
public static fun transpose(m: GpuMat3): GpuMat3
```

## `transpose`

Transpose of a `GpuMat4` (WGSL `transpose`).

```dream
public static fun transpose(m: GpuMat4): GpuMat4
```

## `distance`

Euclidean distance between two scalar values: ` / a - b / ` (WGSL `distance`).

```dream
public static fun distance(a: float, b: float): float
```

## `refract`

Refraction vector for incident ray `i`, surface normal `n`, and index of refraction ratio `eta` (WGSL `refract`).

```dream
public static fun refract(i: GpuVec3, n: GpuVec3, eta: float): GpuVec3
```

## `faceforward`

Flips the surface normal `n` to face the opposite direction of incident vector `i` if `dot(nref, i) >= 0` (WGSL `faceForward`).

```dream
public static fun faceforward(n: GpuVec3, i: GpuVec3, nref: GpuVec3): GpuVec3
```

## `exp2`

Base-2 exponential `2^x` (WGSL `exp2`).

```dream
public static fun exp2(x: float): float
```

## `log2`

Base-2 logarithm `log2(x)` (WGSL `log2`).

```dream
public static fun log2(x: float): float
```

## `round`

Rounds to the nearest integer, rounding ties to the nearest even number (WGSL `round`).

```dream
public static fun round(x: float): float
```

## `trunc`

Truncates fractional digits toward zero (WGSL `trunc`).

```dream
public static fun trunc(x: float): float
```

## `radians`

Converts an angle from degrees to radians: `deg * (pi / 180)` (WGSL `radians`).

```dream
public static fun radians(deg: float): float
```
