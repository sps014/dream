# String

**Import:** `import system.text;`

Read the [usage guide](../stdlib/string.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](text-string.md)

## `last_index_of`

The index of the last occurrence of substring `sub`, or `None`. Empty `sub` is `Some(length)`. Byte-compare scan like `index_of` — allocates nothing per candidate.

```dream
public fun last_index_of(borrow sub: string): Option<int>
```

## `replace_first`

Replaces the first occurrence of `old` with `replacement`.

```dream
public fun replace_first(borrow old: string, borrow replacement: string): string
```

## `split_once`

Splits at the first occurrence of `sep` into exactly two parts. When `sep` does not occur, returns `[this]` (consistent with `split`), never an empty array.

```dream
public fun split_once(sep: char): string[]
```

## `split_once`

```dream
public fun split_once(borrow sep: string): string[]
```

## `strip_prefix`

`Some` remainder when this starts with `prefix`.

```dream
public fun strip_prefix(borrow prefix: string): Option<string>
```

## `strip_suffix`

`Some` remainder when this ends with `suffix`.

```dream
public fun strip_suffix(borrow suffix: string): Option<string>
```

## `lines`

Splits on `\n` and strips a trailing `\r` from each part.

```dream
public fun lines(): string[]
```

## `repeat`

A new string repeated `times` times.

```dream
public fun repeat(times: int): string
```

## `equals`

Value equality (`Equatable<string>`); identical to `this == other`.

```dream
public fun equals(borrow other: string): bool
```

## `split`

Splits this string on the single character `sep`.

```dream
public fun split(sep: char): string[]
```

## `split`

Splits this string on every occurrence of substring `sep`.

```dream
public fun split(borrow sep: string): string[]
```

## `split`

Splits on `sep` into at most `limit` parts (`limit < 1` means no limit).

```dream
public fun split(sep: char, limit: int): string[]
```

## `split`

Splits on `sep` into at most `limit` parts (`limit < 1` means no limit).

```dream
public fun split(borrow sep: string, limit: int): string[]
```

## `replace`

A new string with every occurrence of `old` replaced by `replacement`.

```dream
public fun replace(borrow old: string, borrow replacement: string): string
```

## `join`

Joins elements with `sep` between them.

```dream
public fun join(sep: string): string
```
