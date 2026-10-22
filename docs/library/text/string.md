---
title: Strings
summary: "Join, interpolate, search, and change text."
icon: material/format-quote-close
api: reference/api/system-text.md
---

# Strings

Strings hold text. Join text with `+`, insert values with string interpolation, or use the helpers below to search and change it. The stored text uses UTF-16; use graphemes when you need to work with visible characters.

```dream
import system;

fun main() {
    let name = "Ada";
    System.println("Hello, " + name);
    System.println($"hi {name}");
    System.println("hello".to_upper());
}
```

`.length` counts UTF-16 code units, not Unicode scalars. Use `.byte_size()` for the raw UTF-16 payload size (`length * 2`). Every empty string literal `""` shares one allocation.

## Look up and change

| Call | Meaning |
| --- | --- |
| `s[i]` | UTF-16 code unit at index |
| `s[i] = c` | replace a code unit |
| `byte_at(i)` | one payload byte (UTF-16 LE) |
| `for (let c in s)` | walk code units |

## Search and transform

| Call | Meaning |
| --- | --- |
| `contains` / `starts_with` / `ends_with` | tests |
| `index_of(char or string)` / `last_index_of` | `Option<int>` |
| `split(sep)` / `split(sep, limit)` / `split_once` | `string[]` |
| `replace(old, new)` / `replace_first` | new string |
| `trim` / `trim_start` / `trim_end` / `is_blank` / `repeat(n)` | whitespace / repeat |
| `pad_start` / `pad_end` | pad to a width |
| `strip_prefix` / `strip_suffix` | `Option<string>` |
| `substring(start, end)` | slice |
| `to_lower` / `to_upper` | ASCII case |
| `to_lower_unicode` / `to_upper_unicode` | full Unicode case |
| `normalize(form)` / `graphemes()` | Unicode normalize / grapheme clusters |
| `equals` / `compare` | equality / ordering |

The Unicode helpers follow Unicode 16.0 and give the same result on every target, including the browser. Case mapping does not depend on a locale: `"İ".to_lower_unicode()` is `"i̇"` everywhere, and a capital sigma at the end of a word lowercases to `ς`. Full case mapping can change the length, as in `"straße".to_upper_unicode() == "STRASSE"`.

## Views without allocating

`substring` returns a new `string`. When you only need to look at part of a string (to compare it, parse it, or look it up in a map), take a `ReadOnlySpan<char>` instead. `s.span(start, end)` clamps like `substring`, but never allocates:

```dream
let line = "port=8080";
let value = line.span(5, line.length);
System.println(value.parse_int().unwrap_or(0)); // 8080
```

Compare a span with a string using `sp == "text"` or `"text" == sp`; `!=` works in both orders too. These compare contents without allocating.

`split_iter(sep)` and `lines()` walk the pieces as spans, and `Map<string, V>` / `Set<string>` accept a span key. Call `to_string()` when you need an owned copy. See [Spans](../../language/types/spans.md).

## `StringBuilder`

Grow a string without a new allocation on every `+`:

```dream
let b = StringBuilder();
b.append("hello");
b.append_line(" world");
System.println(b.build());
```

Also: `append_char`, `append_int`, `append_long`, `append_double`, `append_bool`, `.length`, `is_empty()`, `clear()`, `build()`. `append_int` writes decimal digits into the builder (no intermediate `to_string()` allocation).

## Join an array of strings

`parts.join(separator)` combines a string array into one string. For example, `["Ada", "Sam"].join(", ")` returns `"Ada, Sam"`. Use [List.join](../collections/list.md) when you have a list instead.

## Choose a Unicode normalization form

The same visible text can be represented by different sequences of characters. Normalization gives it a consistent representation before comparison or storage.

| Form | What it does |
| --- | --- |
| `UnicodeNormForm.Nfc` | Combines characters where a standard combined form exists |
| `UnicodeNormForm.Nfd` | Separates combined characters into their standard components |
| `UnicodeNormForm.Nfkc` | Also replaces compatibility forms, such as presentation variants |
| `UnicodeNormForm.Nfkd` | Applies compatibility replacements and separates components |

Use `text.normalize(UnicodeNormForm.Nfc)`. Compatibility forms can change distinctions in the original text; choose them deliberately.

`graphemes()` returns the pieces a reader sees as characters. This differs from `.length`, which counts the stored UTF-16 units. Use graphemes when splitting visible text, especially text containing accents or emoji.

For reusable message patterns, read [Formatting](formatting.md). Exact methods and overloads are in the [string reference](../../reference/api/text-string-text.md), [normalization forms reference](../../reference/api/text-unicode-norm-form.md), and [StringBuilder reference](../../reference/api/core-string-builder.md).

`StringBuilder.append_utf8_slice(text, start, byte_len)` is an advanced method: its positions and length refer to the string's stored payload bytes. Use ordinary `append` for whole strings. An invalid nonempty byte range stops the program.
