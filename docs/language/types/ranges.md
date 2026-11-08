---
title: Ranges
summary: "Reusable integer range values with exclusive ends."
icon: material/arrow-expand-horizontal
status: stable
targets: [native, web, node]
---

# Ranges

`start..end` creates a reusable `Range` with `int` bounds and an excluded end. The operator needs no import; this example imports `system` only to print:

```dream
import system;

fun main(): void {
    let bounds = 2..5;
    System.println(bounds.start); // 2
    System.println(bounds.end); // 5
    System.println(bounds.length); // 3
    System.println(bounds.contains(4)); // true
    System.println(bounds.contains(5)); // false
    System.println(bounds.is_empty()); // false
}
```

## Direction and bounds

A range moves toward its end in steps of one:

- `0..5` contains `0, 1, 2, 3, 4`.
- `5..0` contains `5, 4, 3, 2, 1`.
- `5..5` is empty.

A reversed range counts downward. `0..n` with a negative `n` contains `abs(n)` values.

Bounds can be expressions: `-1..n - 1` means `(-1)..(n - 1)`. Both bounds must be `int`, and they are evaluated once, left to right, when the range is created.

There is no inclusive `..=` form and no custom step. [Switch range patterns](enums-unions.md#or-patterns-and-range-patterns) match both bounds inclusively and do not build a `Range`.

`length` is the distance between the bounds, and it panics if that distance exceeds `2147483647`. Iteration itself allows any `int` bounds.

`contains(value)` follows the direction: `(5..0).contains(5)` is true, and `(5..0).contains(0)` is false.

## Iteration

A `for` loop visits the values in order. Each traversal is independent; changing
a stored range during a loop does not change that loop's bounds.

```dream
import system;

fun main(): void {
    let bounds = 3..0;
    for let value in bounds {
        System.println(value);
    }
    for let value in bounds {
        System.println(value);
    }
}
```

Each loop prints `3, 2, 1`, one number per line. `break` and `continue` work as
in other [loops](../basics/control-flow.md#loops).

## Converting to a list

Import `system.collections` to copy a range into a new list in iteration order:

```dream
import system;
import system.collections;

fun main(): void {
    let values = (5..0).to_list();
    System.println(values[0]); // 5
    System.println(values.length); // 5
}
```

The list has independent storage. `to_list()` has the same length restriction as
`length`. See the [Range API](../../reference/api/core-range.md) for its members.
