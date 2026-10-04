# StringBuilder

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class StringBuilder`

`StringBuilder` — growable UTF-16 string construction. Lives in the bootstrap core so every package (collections included) can build strings in O(n); always available without imports.

```dream
public class StringBuilder
```

## `constructor`

Allocates an empty builder with an initial backing buffer capacity in payload bytes (defaults to 16).

```dream
public constructor(capacity: int = 16)
```

## `length`

Number of UTF-16 code units appended so far (`count` is always even).

```dream
public get length(): int
```

## `is_empty`

True when nothing has been appended yet.

```dream
public fun is_empty(): bool
```

## `append_char`

Appends a Unicode scalar as one or two UTF-16 units.

```dream
public fun append_char(c: char): void
```

## `append`

Appends every UTF-16 unit of `text`, growing at most once.

```dream
public fun append(borrow text: string): void
```

## `append_utf8_slice`

Appends `byte_len` payload bytes starting at byte index `start` of `text`.

```dream
public fun append_utf8_slice(borrow text: string, start: int, byte_len: int): void
```

## `append_int`

Appends a decimal integer without allocating an intermediate string.

```dream
public fun append_int(n: int): void
```

## `append_bool`

Appends `"true"` or `"false"` without allocating an intermediate string.

```dream
public fun append_bool(v: bool): void
```

## `append_long`

Appends a decimal long without allocating an intermediate string.

```dream
public fun append_long(n: long): void
```

## `append_double`

Appends a double via `to_string`.

```dream
public fun append_double(n: double): void
```

## `append_line`

Appends `text` followed by a newline character.

```dream
public fun append_line(borrow text: string): void
```

## `clear`

Removes every appended character, keeping the backing buffer for reuse.

```dream
public fun clear(): void
```

## `build`

Snapshot into a new string.

```dream
public fun build(): string
```

## `to_string`

```dream
public override fun to_string(): string
```
