# GpuMath

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-math.md)

## `degrees`

Converts an angle from radians to degrees: `rad * (180 / pi)` (WGSL `degrees`).

```dream
public static fun degrees(rad: float): float
```

## `determinant`

Determinant of a 2x2 matrix (WGSL `determinant`).

```dream
public static fun determinant(m: GpuMat2): float
```

## `determinant`

Determinant of a 3x3 matrix (WGSL `determinant`).

```dream
public static fun determinant(m: GpuMat3): float
```

## `determinant`

Determinant of a 4x4 matrix (WGSL `determinant`).

```dream
public static fun determinant(m: GpuMat4): float
```

## `inverse`

Multiplicative inverse of a 2x2 matrix: `m^-1` such that `m * m^-1 = I`. WGSL has no `inverse` builtin; shaders lower this to adjugate / `determinant`.

```dream
public static fun inverse(m: GpuMat2): GpuMat2
```

## `inverse`

Multiplicative inverse of a 3x3 matrix (WGSL `inverse`).

```dream
public static fun inverse(m: GpuMat3): GpuMat3
```

## `inverse`

Multiplicative inverse of a 4x4 matrix (WGSL `inverse`).

```dream
public static fun inverse(m: GpuMat4): GpuMat4
```

## `pack4x8unorm`

Packs four normalized floats into the four bytes of a 32-bit integer (WGSL `pack4x8unorm`). Each component is clamped to [0, 1] and scaled by 255, with `x` in the low byte. Use it to halve the size of vertex colours and normals before upload; the shader-side `unpack4x8unorm` reverses it exactly. These run on the CPU as well as in a shader, so packing a mesh on the host and unpacking it in a shader agree bit for bit.

```dream
public static fun pack4x8unorm(v: GpuVec4): int
```

## `pack4x8snorm`

Packs four signed normalized floats into a 32-bit integer (WGSL `pack4x8snorm`). Each component is clamped to [-1, 1] and scaled by 127.

```dream
public static fun pack4x8snorm(v: GpuVec4): int
```

## `pack2x16unorm`

Packs two normalized floats into the halves of a 32-bit integer (WGSL `pack2x16unorm`). Each component is clamped to [0, 1] and scaled by 65535, with `x` in the low half.

```dream
public static fun pack2x16unorm(v: GpuVec2): int
```

## `pack2x16snorm`

Packs two signed normalized floats into a 32-bit integer (WGSL `pack2x16snorm`). Each component is clamped to [-1, 1] and scaled by 32767.

```dream
public static fun pack2x16snorm(v: GpuVec2): int
```

## `unpack4x8unorm`

Reverses `pack4x8unorm` (WGSL `unpack4x8unorm`).

```dream
public static fun unpack4x8unorm(v: int): GpuVec4
```

## `unpack4x8snorm`

Reverses `pack4x8snorm` (WGSL `unpack4x8snorm`). -128 decodes to -1, matching WGSL's clamp of the low end.

```dream
public static fun unpack4x8snorm(v: int): GpuVec4
```

## `unpack2x16unorm`

Reverses `pack2x16unorm` (WGSL `unpack2x16unorm`).

```dream
public static fun unpack2x16unorm(v: int): GpuVec2
```

## `unpack2x16snorm`

Reverses `pack2x16snorm` (WGSL `unpack2x16snorm`).

```dream
public static fun unpack2x16snorm(v: int): GpuVec2
```

## `count_one_bits`

Number of 1 bits in the 32-bit integer binary representation (WGSL `countOneBits` / popcount). The bit functions all go through `uint` so a negative `int` shifts in zeros rather than sign bits, matching how WGSL counts over a `u32`.

```dream
public static fun count_one_bits(x: int): int
```

## `reverse_bits`

Reverses the order of the bits in a 32-bit integer (WGSL `reverseBits`).

```dream
public static fun reverse_bits(x: int): int
```

## `count_leading_zeros`

Number of consecutive zero bits starting from the most significant bit (WGSL `countLeadingZeros` / clz).

```dream
public static fun count_leading_zeros(x: int): int
```

## `count_trailing_zeros`

Number of consecutive zero bits starting from the least significant bit (WGSL `countTrailingZeros` / ctz).

```dream
public static fun count_trailing_zeros(x: int): int
```
