# String Text

**Import:** `import system.text;`

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

Replaces the UTF-16 code unit at `index`; an invalid index panics.

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

A new string with every character lowercased by the Unicode full case mapping, which can change the length (`"İ"` becomes `"i̇"`). Capital sigma becomes final sigma `ς` at the end of a word. The result is the same on every target and does not depend on a locale.

```dream
public fun to_lower_unicode(): string
```

## `to_upper_unicode`

A new string with every character uppercased by the Unicode full case mapping, which can change the length (`"straße"` becomes `"STRASSE"`). The result is the same on every target and does not depend on a locale.

```dream
public fun to_upper_unicode(): string
```

## `graphemes`

Splits the string into extended grapheme clusters (Unicode UAX #29), the units a reader perceives as single characters: a letter with its accents, an emoji sequence joined by zero-width joiners, or a flag. Returns an empty array for an empty string.

```dream
public fun graphemes(): string[]
```

## `normalize`

Returns the string in Unicode normalization `form` (UAX #15). `Nfc` and `Nfd` preserve meaning; `Nfkc` and `Nfkd` also replace compatibility characters such as `ﬁ` with `fi`.

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
