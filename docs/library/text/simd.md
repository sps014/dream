---
title: SIMD
summary: Process a small group of numbers with each operation.
icon: material/vector-combine
api: reference/api/system-simd.md
---

# Process values with SIMD

Import `system.simd` so one operation handles a small group of numbers. `Vector<T>` loads lanes, combines them, and stores the result; use an ordinary loop when you do not need lanes.

`T` is `byte`, `int`, `long`, `float`, or `double`, and a vector is 16 bytes (16, 4, 2, 4, or 2 lanes). Call `Vector<T>.lane_count()` instead of hard-coding that count.

## Add two arrays

The first loop handles complete vectors, and the second handles the leftover elements. All three arrays must have the same length; this function checks that before it writes.

```dream
import system;
import system.simd;

fun add(a: float[], b: float[], c: float[]): void {
    let n = a.length;
    if b.length != n || c.length != n {
        System.panic("Array lengths must match");
    }

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

fun main(): void {
    let a: float[] = [1.0f, 2.0f, 3.0f, 4.0f, 5.0f];
    let b: float[] = [10.0f, 20.0f, 30.0f, 40.0f, 50.0f];
    let c = [0.0f; a.length];

    add(a, b, c);

    System.println(c[0]);
    System.println(c[4]);
}
```

Save this as `vectors.dream` and run `dream run vectors.dream`. The array is 11, 22, 33, 44, 55 (first and last are printed), covering one full vector and a one-element remainder.

## Operations

| API | Meaning |
| --- | --- |
| `Vector<T>(value)` / `splat` | Fill every lane with `value` |
| `load` / `store` | Read or write one vector at an array offset |
| `lane` / `with_lane` | Read or replace one lane |
| `+`, `-`, `*` | Combine corresponding lanes |
| `min` / `max` | Select the smaller or larger value in each pair of lanes |
| `sum` | Add all lanes and return one value |

A load or store needs enough elements from the requested offset to hold every lane.
Keep the vector loop's bound check and handle the remainder separately, as above.
