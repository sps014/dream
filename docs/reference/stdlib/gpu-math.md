# Vectors and matrices

Use vector and matrix helpers to express positions, directions, and transformations.

[Back to overview](gpu.md)

## Vector math

`GpuVec2` / `GpuVec3` / `GpuVec4` are packed float vectors (`vecN<f32>` in WGSL). The
same operators work in `@compute` / `@vertex` / `@fragment` and on the CPU:

| Expression | Meaning (WGSL) |
| --- | --- |
| `v + w`, `v - w`, `v * w`, `v / w` | component-wise |
| `v * s`, `s * v`, `v / s` | scale / divide by `float` |
| `s + v`, `v + s`, `v - s`, `s - v`, `s / v` | scalar on either side |
| `-v` | negate |
| `m * v`, `m * n` | `GpuMatN` × vector / matrix |
| `GpuVecN.of(...)` | `vecN(x, y, …)` |
| `GpuVecN.splat(s)` | `vecN(s)` |
| `GpuMatN.of(c0, …)` | `matNxN(c0, …)` column-major |
| `GpuMatN.identity()` | identity matrix |
| `GpuMat4.perspective(fov_y, aspect, near, far)` | WebGPU clip Z in `[0, 1]`, `fov_y` in radians |
| `GpuMat4.ortho(l, r, b, t, near, far)` | orthographic projection |
| `GpuMat4.look_at(eye, center, up)` | right-handed view matrix |
| `GpuMat4.translation` / `rotation` / `scaling` | TRS builders |
| `GpuMat3.normal_matrix(m)` | inverse-transpose of `m`'s upper 3×3 |
| `GpuQuat.xyzw` / `from_axis_angle` / `rotate` / `to_mat4` | CPU-side rotation (turn into a matrix for shaders) |

`GpuMath` overloads (same names as the scalar builtins):

| Call | Vector args |
| --- | --- |
| `mix(a, b, t)` | `vec, vec, float` or `vec, vec, vec` |
| `min` / `max` | `vec, vec` |
| `abs` / `sign` / `floor` / `ceil` / `fract` / `sqrt` / `exp` / `exp2` / `log2` / `round` / `trunc` / `radians` / `degrees` / `saturate` | `vec` |
| `clamp(x, lo, hi)` | `vec, float, float` or `vec, vec, vec` |
| `pow(x, e)` | `vec, float` or `vec, vec` |
| `normalize` / `length` / `dot` | `GpuVec2` / `GpuVec3` / `GpuVec4` |
| `distance(a, b)` | `GpuVec2` / `GpuVec3` / `GpuVec4` |
| `refract(i, n, eta)` / `faceforward(n, i, nref)` | `GpuVec3` |
| `transpose` / `inverse` | `GpuMat2` / `GpuMat3` / `GpuMat4` |
| `determinant` | `GpuMat2` / `GpuMat3` / `GpuMat4` |
| `mul` | `GpuMatN × GpuVecN` or `GpuMatN × GpuMatN` |
| `count_one_bits` / `reverse_bits` / `count_leading_zeros` / `count_trailing_zeros` | `int` bitwise |

Near-zero `normalize` on the CPU returns a unit axis (`(1,0)`, `(0,1,0)`, or `(0,0,0,1)`);
shaders use WGSL `normalize`.

`dream run` uses the machine's GPU. The browser uses the page's GPU. More samples: [`life/`](https://github.com/sps014/dream/tree/main/sample/compute/life), [`fluid/`](https://github.com/sps014/dream/tree/main/sample/fluid), [`ocean/`](https://github.com/sps014/dream/tree/main/sample/graphics/ocean), [`elevated/`](https://github.com/sps014/dream/tree/main/sample/graphics/elevated).
