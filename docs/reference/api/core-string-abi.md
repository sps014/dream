# String Abi

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `alloc`

Allocates an uninitialized string with capacity for `n` UTF-16 code units.

```dream
public static extern fun alloc(n: int): string
```

## `set`

Writes code unit `c` at unit index `i` of string `s`.

```dream
public static extern fun set(borrow s: string, i: int, c: char): void
```

## `from_utf8`

Builds a string from a full `byte[]` UTF-8 payload (transcodes to UTF-16).

```dream
public static extern fun from_utf8(borrow bytes: byte[]): string
```

## `from_utf8_prefix`

Builds a string from the first `len` bytes of `bytes` (clamped to the array length).

```dream
public static extern fun from_utf8_prefix(borrow bytes: byte[], len: int): string
```

## `from_utf8_prefix_n`

Copies `len` UTF-16 LE payload bytes from `bytes`; `scalars` is the unit count.

```dream
public static extern fun from_utf8_prefix_n(borrow bytes: byte[], len: int, scalars: int): string
```

## `from_builder`

StringBuilder finish: unique buffers (`rc == 1`) are adopted; otherwise copied.

```dream
public static extern fun from_builder(borrow bytes: byte[], len: int, scalars: int): string
```

## `substring_raw`

Unit-indexed substring: O(1) slice (header + payload pointer + retain parent).

```dream
public static extern fun substring_raw(borrow s: string, start: int, end: int): string
```

## `clone`

Fresh heap copy of `s` (isolation boundary for `@shared` string fields / worker captures).

```dream
public static extern fun clone(borrow s: string): string
```

## `copy_utf8`

Bulk-copies `count` payload bytes from `src` into `dst` at `dst_off`.

```dream
public static extern fun copy_utf8(
    borrow dst: byte[],
    dst_off: int,
    borrow src: string,
    src_off: int,
    count: int
): void
```

## `compare_raw`

UTF-16 lexicographic compare (code-unit order): negative / zero / positive.

```dream
public static extern fun compare_raw(borrow a: string, borrow b: string): int
```

## `utf8_decode_at`

Unchecked UTF-16 unit load at a payload byte offset (even offsets).

```dream
public static extern fun utf8_decode_at(borrow s: string, byte_off: int): char
```

## `utf8_width_at`

Payload stride in bytes of the unit at `byte_off` (always 2).

```dream
public static extern fun utf8_width_at(borrow s: string, byte_off: int): int
```

## `empty`

The interned empty string. Prefer this over `""` so empty results share one immortal pointer instead of a fresh heap block (substring/from_utf8/concat of empties also return this).

```dream
public static get empty(): string
```
