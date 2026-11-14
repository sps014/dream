# String Unicode

**Import:** `import system.unicode;`

Read the [usage guide](../../library/text/string.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
