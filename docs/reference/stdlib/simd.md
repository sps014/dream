# SIMD (`system.simd`)

**Import:** `import system.simd;`

A plain loop can use vector math; `Vector` is for when you want lanes yourself.

1. **Autovectorization** — a counted loop like `c[i] = a[i] + b[i]`, when every index is in range, runs wide loads and stores. If `n` is not a multiple of the lane count, the leftover elements run one at a time.
2. **`Vector<T>`** — explicit portable SIMD when you load, operate, and store lanes yourself.

`T` must be `byte`, `int`, `long`, `float`, or `double`. `Vector<T>.lane_count()` is `16 / sizeof(T)` (16, 4, 2, 4, 2).

```dream
import system;
import system.simd;

fun add(a: float[], b: float[], c: float[]): void {
    let n = a.length;
    let lanes = Vector<float>.lane_count();
    let i = 0;
    while i + lanes <= n {
        (Vector<float>.load(a, i) + Vector<float>.load(b, i)).store(c, i);
        i = i + lanes;
    }
    while i < n {
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

A plain counted loop can use vector math on its own. You do not need `Vector` for that.
