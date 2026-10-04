# Buffer

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Buffer`

This is the raw primitive the higher-level `List`/`Map`/`Set` collections build on; user code should normally prefer `List<T>` and only reach for `Buffer.alloc` for tight, manual storage control.

```dream
public static class Buffer
```

## `clear`

Releases the element in every slot of `arr` by overwriting it with a zero value, so a container can rewind its length without stranding retained elements in dead slots. This is the sanctioned way to shrink a managed buffer: each store goes through normal ARC assignment semantics (release old occupant), unlike bitwise fills which must never touch reference-typed slots. O(length); unmanaged elements compile to plain stores.

```dream
public static fun clear<T>(borrow arr: T[]): void
```

## `truncate`

Like `clear`, but releases only slots `[n, arr.length)` — the tail dropped by a shrink to `n`. Slots below `n` are left untouched.

```dream
public static fun truncate<T>(borrow arr: T[], n: int): void
```
