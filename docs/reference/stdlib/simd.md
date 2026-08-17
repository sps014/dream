# SIMD (`system.simd`)

**Import:** `import system.simd;`

Dream has two SIMD stories on WASM `v128` (16 bytes):

1. **Autovectorization** — a counted `c[i] = a[i] + b[i]` loop (ABC-proven `unchecked` indexes) is rewritten to `v128` loads/stores plus a scalar remainder when `n` is not a multiple of the lane count.
2. **`Vector<T>`** — explicit portable SIMD, comparable to `System.Numerics.Vector<T>` in C#.

`T` must be `byte`, `int`, `long`, `float`, or `double`. `Vector<T>.count()` is `16 / sizeof(T)` (16, 4, 2, 4, 2).

```dream
import system;
import system.simd;

fun add(a: float[], b: float[], c: float[]): void {
    let n = a.length;
    let lanes = Vector<float>.count();
    let i = 0;
    while (i + lanes <= n) {
        (Vector<float>.load(a, i) + Vector<float>.load(b, i)).store(c, i);
        i = i + lanes;
    }
    while (i < n) {
        c[i] = a[i] + b[i];
        i = i + 1;
    }
}
```

| API | Meaning |
| --- | --- |
| `Vector<T>(value)` / `splat` | every lane set to `value` |
| `load` / `store` | `count` elements at an array offset |
| `lane` / `with_lane` | extract / replace one lane |
| `+` `-` `*` | lane-wise arithmetic |
| `min` / `max` | lane-wise min/max |
| `sum` | horizontal sum |

Owning `Vector<T>` locals are WASM `v128` values (not four heap words or a 16-byte sret shadow slot). `load` / lane arithmetic / `store` are `v128.load`, `f32x4.add` (or the matching lane op), and `v128.store`. Cross-function `Vector` parameters and returns still use the value-struct sret pointer and are copied into a `v128` local at the callee prologue.

Autovec does **not** require `Vector`. Users write ordinary indexed loops; the compiler emits `f32x4.add` / `i32x4.add` (and the other lane ops) when the loop is sound.
