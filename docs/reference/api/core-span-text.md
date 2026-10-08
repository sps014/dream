# Character view operations

No import is needed.

Read the [span guide](../language/spans.md) for usage and backing-storage semantics.

## `char_at`

```dream
public fun char_at(index: int): char
```

## `byte_size`

String payloads use UTF-16; character arrays store full 32-bit values.

```dream
public fun byte_size(): int
```

## `byte_at`

```dream
public fun byte_at(index: int): int
```

## `to_string`

```dream
public override fun to_string(): string
```

## `hash_code`

Matches string hashing for UTF-16 elements without allocating a string.

```dream
public override fun hash_code(): int
```

## `equals`

```dream
public fun equals(borrow other: string): bool
```

## `compare`

```dream
public fun compare(other: ReadOnlySpan<T>): int
```

## `compare`

```dream
public fun compare(borrow other: string): int
```

## `starts_with`

```dream
public fun starts_with(borrow prefix: string): bool
```

## `ends_with`

```dream
public fun ends_with(borrow suffix: string): bool
```

## `trim`

```dream
public fun trim(): ReadOnlySpan<T>
```

## `trim_start`

```dream
public fun trim_start(): ReadOnlySpan<T>
```

## `trim_end`

```dream
public fun trim_end(): ReadOnlySpan<T>
```

## `parse_int`

```dream
public fun parse_int(): Result<int, ParseError>
```

## `parse_double`

```dream
public fun parse_double(): Result<double, ParseError>
```

## `split_iter`

```dream
public fun split_iter(sep: char): StringSplit
```

## `split_iter`

```dream
public fun split_iter(borrow sep: string): StringSplit
```

## `lines`

```dream
public fun lines(): StringLines
```
