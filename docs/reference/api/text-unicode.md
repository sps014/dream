# Unicode

**Import:** `import system.text;`

Read the [usage guide](../stdlib/string.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Unicode`

ICU-class Unicode helpers (normalization, grapheme segmentation, full case folding).

```dream
public static class Unicode
```

## `normalize`

Normalizes `text` to the requested Unicode form.

```dream
public static fun normalize(text: string, form: UnicodeNormForm): string
```

## `to_lower_unicode`

Full Unicode lowercase (not limited to ASCII).

```dream
public static fun to_lower_unicode(text: string): string
```

## `to_upper_unicode`

Full Unicode uppercase (not limited to ASCII).

```dream
public static fun to_upper_unicode(text: string): string
```

## `graphemes`

Splits `text` into user-perceived grapheme clusters.

```dream
public static fun graphemes(text: string): string[]
```
