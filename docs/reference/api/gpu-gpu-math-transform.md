# GpuQuat

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `perspective`

Perspective projection. `fov_y` is the vertical field of view in radians; `near` / `far` are positive distances. Degenerate inputs (non-positive fov/aspect, `near == far`) return the identity so a bad camera does not produce NaNs that poison a whole frame.

```dream
public static fun perspective(fov_y: float, aspect: float, near: float, far: float): GpuMat4
```

## `ortho`

Orthographic projection mapping `left..right`, `bottom..top`, `near..far` into clip space.

```dream
public static fun ortho( left: float, right: float, bottom: float, top: float, near: float, far: float ): GpuMat4
```

## `look_at`

View matrix looking from `eye` at `center`, with world-up `up`. Right-handed: the camera looks down −Z in view space.

```dream
public static fun look_at(eye: GpuVec3, center: GpuVec3, up: GpuVec3): GpuMat4
```

## `translation`

Translation by `t`.

```dream
public static fun translation(t: GpuVec3): GpuMat4
```

## `rotation`

Rotation of `radians` about unit `axis` (Rodrigues). A near-zero axis returns identity.

```dream
public static fun rotation(axis: GpuVec3, radians: float): GpuMat4
```

## `scaling`

Non-uniform scale.

```dream
public static fun scaling(s: GpuVec3): GpuMat4
```

## `normal_matrix`

Inverse-transpose of the upper-left 3×3 of `m`, for transforming normals. Singular input returns identity.

```dream
public static fun normal_matrix(m: GpuMat4): GpuMat3
```

## `struct GpuQuat`

Unit quaternion for 3D rotation. CPU-side (camera, animation); converting to a `GpuMat4` is the path into a shader. Stored as `(x, y, z, w)` with `w` the real part.

```dream
public struct GpuQuat
```

## `x: float`

```dream
public x: float
```

## `y: float`

```dream
public y: float
```

## `z: float`

```dream
public z: float
```

## `w: float`

```dream
public w: float
```

## `xyzw`

```dream
public static fun xyzw(x: float, y: float, z: float, w: float): GpuQuat
```

## `identity`

```dream
public static fun identity(): GpuQuat
```

## `from_axis_angle`

Rotation of `radians` about `axis`.

```dream
public static fun from_axis_angle(axis: GpuVec3, radians: float): GpuQuat
```

## `mul`

```dream
public fun mul(other: GpuQuat): GpuQuat
```

## `conjugate`

```dream
public fun conjugate(): GpuQuat
```

## `length`

```dream
public fun length(): float
```

## `normalize`

```dream
public fun normalize(): GpuQuat
```

## `rotate`

Rotate a vector by this quaternion.

```dream
public fun rotate(v: GpuVec3): GpuVec3
```

## `to_mat4`

```dream
public fun to_mat4(): GpuMat4
```
