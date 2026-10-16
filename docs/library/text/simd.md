---
title: SIMD
summary: Process a small group of numbers with each operation.
icon: material/vector-combine
api: reference/api/system-simd.md
---

# Process values with SIMD

Import `system.simd` to process a small group of numbers with each operation.
`Vector<T>` loads values from an array, combines corresponding lanes, and stores the
result back into an array. Use an ordinary loop when direct control over lanes is
unnecessary.

`T` must be `byte`, `int`, `long`, `float`, or `double`. A vector holds 16 bytes:
16 byte lanes, 4 int lanes, 2 long lanes, 4 float lanes, or 2 double lanes.
Use `Vector<T>.lane_count()` rather than hard-coding the count in a loop.

## Add two arrays

The first loop handles complete vectors. The second handles any remaining elements.
All three arrays must have the same length; this function checks that requirement
before writing the output.

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
    let c = Array.alloc<float>(a.length);

    add(a, b, c);

    System.println(c[0]);
    System.println(c[4]);
}
```

Save this as `vectors.dream` and run `dream run vectors.dream`. The result array
contains 11, 22, 33, 44, and 55; the program prints its first and last values.
Five inputs exercise both a four-lane vector and the one-element remainder.

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
