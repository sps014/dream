# String

**Import:** `import system.text;`

Read the [usage guide](../stdlib/string.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](text-string.md)

## `compare`

Orders this string against `other` lexicographically.

```dream
public fun compare(borrow other: string): int
```

## `is_empty`

True when the string has no characters.

```dream
public fun is_empty(): bool
```

## `this`

Gets the character at the specified index.

```dream
public fun this[index: int]: char
```

## `this`

Overwrites the character at the specified index (panics if out of range). Named `set_at` (not `set`) so it does not collide with static `string.set`.

```dream
public fun this[index: int] = value: char
```

## `iterator`

Gets an iterator for the string's characters.

```dream
public fun iterator(): StringIterator
```

## `substring`

Gets a substring from start to end (O(1) slice of the parent).

```dream
public fun substring(start: int, end: int): string
```

## `index_of`

The index of the first occurrence of `target`, or `None`.

```dream
public fun index_of(target: char): Option<int>
```

## `index_of`

The index of the first occurrence of substring `sub`, or `None`.

```dream
public fun index_of(borrow sub: string): Option<int>
```

## `contains`

True if `sub` occurs anywhere in this string.

```dream
public fun contains(borrow sub: string): bool
```

## `starts_with`

True if this string begins with `prefix`.

```dream
public fun starts_with(borrow prefix: string): bool
```

## `ends_with`

True if this string ends with `suffix`.

```dream
public fun ends_with(borrow suffix: string): bool
```

## `to_lower`

A new string with every uppercase letter lowercased.

```dream
public fun to_lower(): string
```

## `to_upper`

A new string with every lowercase letter uppercased.

```dream
public fun to_upper(): string
```

## `to_lower_unicode`

Full Unicode lowercase via `Unicode.to_lower_unicode`.

```dream
public fun to_lower_unicode(): string
```

## `to_upper_unicode`

Full Unicode uppercase via `Unicode.to_upper_unicode`.

```dream
public fun to_upper_unicode(): string
```

## `graphemes`

User-perceived grapheme clusters via `Unicode.graphemes`.

```dream
public fun graphemes(): string[]
```

## `normalize`

Unicode normalization to the requested form.

```dream
public fun normalize(form: UnicodeNormForm): string
```

## `trim`

A new string with leading and trailing whitespace removed.

```dream
public fun trim(): string
```

## `trim_start`

A new string with leading whitespace removed.

```dream
public fun trim_start(): string
```

## `trim_end`

A new string with trailing whitespace removed.

```dream
public fun trim_end(): string
```

## `is_blank`

True when `trim()` is empty.

```dream
public fun is_blank(): bool
```

## `pad_start`

Left-pads this string with `pad` until `width` code units (no-op when `pad` is empty).

```dream
public fun pad_start(width: int, pad: string): string
```

## `pad_end`

Right-pads this string with `pad` until `width` code units (no-op when `pad` is empty).

```dream
public fun pad_end(width: int, pad: string): string
```

## `last_index_of`

The index of the last occurrence of `target`, or `None`.

```dream
public fun last_index_of(target: char): Option<int>
```
