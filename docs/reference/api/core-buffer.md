# Buffer

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Buffer`

Allocates and manages fixed-length arrays. Prefer `List<T>` when the number of elements needs to grow.

```dream
public static class Buffer
```

## `alloc`

`Buffer.alloc<T>(n)` returns a fresh, zero-initialized `T[]` of length `n`.

```dream
public static extern fun alloc<T>(len: int): T[]
```

## `realloc`

Resizes an array, preserving the common prefix and zero-initializing newly added elements. The array must have a single owner. Use only the returned array afterward: every old alias becomes invalid.

```dream
public static extern fun realloc<T>(arr: T[], new_len: int): T[]
```

## `elems_copy`

Copies `count` unmanaged elements from `src` at `src_off` to `dst` at `dst_off`. Overlapping ranges are supported. The caller must ensure both ranges fit in their arrays.

```dream
public static extern fun elems_copy<T: unmanaged>(
    borrow dst: T[],
    dst_off: int,
    borrow src: T[],
    src_off: int,
    count: int
): void
```

## `elems_fill`

Zeroes `count` unmanaged elements of `dst` starting at `dst_off`. The caller must ensure that the range fits in the array.

```dream
public static extern fun elems_fill<T: unmanaged>(borrow dst: T[], dst_off: int, count: int): void
```

## `clear`

Resets every array element to its zero value, releasing any references previously held in those elements. The array length is unchanged.

```dream
public static fun clear<T>(borrow arr: T[]): void
```

## `truncate`

Like `clear`, but releases only slots `[n, arr.length)` — the tail dropped by a shrink to `n`. Slots below `n` are left untouched.

```dream
public static fun truncate<T>(borrow arr: T[], n: int): void
```

## `get_unchecked`

`arr[index]` without the bounds check. `@unsafe` because the caller must already have established `0 <= index < arr.length` (e.g. `List<T>` keeps `count <= items.length`).

```dream
public static extern fun get_unchecked<T>(borrow arr: T[], index: int): T
```

## `set_unchecked`

Replaces `arr[index]` without checking bounds. The caller must ensure the index is valid.

```dream
public static extern fun set_unchecked<T>(borrow arr: T[], index: int, value: T): void
```

## `store16`

Writes a UTF-16 code unit as little-endian `i32.store16` at byte offset `off`.

```dream
public static extern fun store16(borrow arr: byte[], off: int, unit: int): void
```

## `free`

Immediately releases the array storage. The array must have a single owner, and neither it nor any view or alias may be used afterward.

```dream
public static extern fun free<T>(arr: T[]): void
```
