# Write rendering functions

Use these supported operations inside your vertex and fragment functions.

[Back to overview](shaders.md)

## Builtins

- Vertex: `vertex_index`, `instance_index`
- Fragment: `frag_coord`, `front_facing`; `sample_index` / `primitive_index` / `sample_mask` when referenced

`sample_mask` is the incoming coverage mask: bit *N* is set when sample *N* of this fragment is covered, so `GpuMath.count_one_bits(sample_mask)` counts covered samples.

An output struct can also **write** `@builtin("sample_mask")` as an `int` field — clearing a bit discards that sample, and clearing every bit discards the fragment:

```dream
struct FsOut {
    @location(0) public color: GpuVec4;
    @builtin("sample_mask") public coverage: int;
}
```


## Control flow

`if` / `while` / `do`-`while` / `for` / `switch` all work, including `break` and `continue`.

Limits:

- **No loop labels.** `break outer;` / `continue outer;` are rejected — `break` and `continue` always apply to the innermost loop. Use a flag local, or move the inner loop into a [`@gpu` helper](#gpu-helpers) and `return` from it.
- **`switch` subjects are evaluated once.** Case labels must be constants: a literal or a simple enum member. Cases do not fall through. A `break` inside a case belongs to the enclosing loop, not to the `switch`.

Simple enums work in shaders. Each member is its integer value:

```dream
enum Mode { Add = 0, Mul = 1, Sub = 2 }

@gpu
fun apply(mode: int, a: float, b: float): float {
    switch (mode) {
        case Mode.Add: return a + b;
        case Mode.Mul: return a * b;
        default: return a - b;
    }
}
```


## Packing

`GpuMath` packs normalized vectors into a 32-bit `int` and back.
These run on the CPU too, so a mesh packed on the host unpacks in a shader to the same bits — useful for halving the size of vertex colours and normals, or for compact G-buffers:

| Pack | Unpack | Component range |
|---|---|---|
| `pack4x8unorm(GpuVec4)` | `unpack4x8unorm(int)` | `[0, 1]` × 4 bytes |
| `pack4x8snorm(GpuVec4)` | `unpack4x8snorm(int)` | `[-1, 1]` × 4 bytes |
| `pack2x16unorm(GpuVec2)` | `unpack2x16unorm(int)` | `[0, 1]` × 2 halves |
| `pack2x16snorm(GpuVec2)` | `unpack2x16snorm(int)` | `[-1, 1]` × 2 halves |

Components are clamped before scaling, and `x` occupies the low bits.


## Vector math

`GpuVec2` / `GpuVec3` / `GpuVec4` support arithmetic inside shaders (and the same expressions on the CPU):

- `v + w`, `v - w`, `v * w`, `v / w` (component-wise)
- `v * s`, `v / s`, `s * v` (and `s + v`, `v + s`, …)
- `-v`
- `GpuMatN * GpuVecN` and `GpuMatN * GpuMatN`

`GpuMath.mix` / `min` / `max` / `abs` / `clamp` / `saturate` / `sign` / `floor` / `ceil` / `fract` / `sqrt` / `exp` / `pow` take `float` or `GpuVecN`. `normalize` / `length` / `dot` use the same name for vec2/3/4. `GpuVecN.splat(s)` builds a vector of all `s`.

`GpuMat2.of(c0, c1)` builds a matrix from columns. `GpuMath.transpose` / `GpuMath.inverse` / `GpuMath.determinant` cover `GpuMat2` / `GpuMat3` / `GpuMat4`. Shader `let` is inferred from the initializer — `let s = GpuMath.sin(t)` needs no `: float`.

### Swizzles

Reading two to four components at once gives a smaller (or reordered) vector:

```dream
let rgb  = color.xyz;   // GpuVec3
let uv   = p.xy;        // GpuVec2
let flip = p.wzyx;      // GpuVec4, reversed
let grey = c.xxx;       // GpuVec3, broadcast
```

`rgba` is an interchangeable spelling for `xyzw` (`color.rgb` is `color.xyz`), but the two cannot be mixed in one name.
Components past the end of the source are an error: `.xyz` on a `GpuVec2` does not compile.

The same expressions work on the CPU, where a swizzle builds a new `GpuVecN` and so reads its value you call the method on once per component.
On the CPU the value you call the method on therefore has to be something re-readable — a local or a field path.
Bind anything else to a local first:

```dream
let n = GpuMath.normalize(v);   // on the CPU, `GpuMath.normalize(v).xyz` is an error
let dir = n.xyz;
```

Inside shaders there is no such restriction — the swizzle evaluates its value you call the method on a single time.

```dream
@gpu
fun shade(a: GpuVec3, b: GpuVec3, t: float): GpuVec3 {
    return GpuMath.mix(a, b, t) * 0.8 + GpuVec3.splat(0.1);
}
```


## `@gpu` helpers

Shaders may only call other GPU stages / `@gpu` helpers and `GpuMath` / `GpuVec*` / `GpuMat*` builtins.
Helpers are **not** callable from CPU code.

```dream
@gpu
fun sea_octave(ux: float, uz: float, choppy: float): float {
    return GpuMath.pow(1.0 - GpuMath.pow(0.5, 0.65), choppy);
}
```

Rules:

- Top-level only
- Not generic / async / extern
- Explicit non-void return type
- Parameters carry values, so a `GpuBuffer` / `GpuTexture` / `GpuSampler` cannot be one — those are bound to a stage, not passed
- Index the resource in the stage function and pass the element, or make the helper its own `@compute` kernel with its own binding


## Matrices

`GpuMat2` / `GpuMat3` / `GpuMat4` (column-major).
Prefer `m * v` / `m * n` in shaders; `GpuMath.mul` is the named form (mat×vec or mat×mat).
`GpuMath.transpose` is available.


## Rules

- Top-level only; not async, generic, or extern.
- `@vertex` returns a value struct with a position builtin plus varyings.
- `@fragment` first parameter is usually that interface struct; return `GpuVec4` or an output struct.
- When names passed to `create` / `create_ex` are **string literals**, Dream checks stages and matching interface types.
- `sizeof(T)` becomes a number in the shader; `nameof(...)` and `typeof(...)` are not available in shader bodies (they produce `string`). Details: [Operators — sizeof, nameof, and typeof](type-queries.md#sizeof-nameof-and-typeof).
