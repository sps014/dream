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

`.length` counts UTF-16 code units, not Unicode scalars. Use `.byte_size()` for the raw UTF-16 payload size (`length * 2`). `string.empty` is the shared empty string.

## Look up and change

| Call | Meaning |
| --- | --- |
| `char_at(i)` / `s[i]` / `get(i)` | UTF-16 code unit at index |
| `s[i] = c` / `set_at(i, c)` | replace a code unit |
| `byte_at(i)` | one payload byte (UTF-16 LE) |
| `for (let c in s)` | walk code units |
| `string.alloc(n)` | allocate `n` code units (low-level) |

## Search and transform

| Call | Meaning |
| --- | --- |
| `contains` / `starts_with` / `ends_with` | tests |
| `index_of(char or string)` / `last_index_of` | `Option<int>` |
| `split(sep)` / `split(sep, limit)` / `split_once` / `lines` | `string[]` |
| `replace(old, new)` / `replace_first` | new string |
| `trim` / `trim_start` / `trim_end` / `is_blank` / `repeat(n)` | whitespace / repeat |
| `pad_start` / `pad_end` | pad to a width |
| `strip_prefix` / `strip_suffix` | `Option<string>` |
| `substring(start, end)` | slice |
| `to_lower` / `to_upper` | ASCII case |
| `to_lower_unicode` / `to_upper_unicode` | full Unicode case |
| `normalize(form)` / `graphemes()` | Unicode normalize / grapheme clusters |
| `equals` / `compare` | equality / ordering |

`Unicode.normalize`, `Unicode.to_lower_unicode`, `Unicode.to_upper_unicode`, and `Unicode.graphemes` are the static forms of the same helpers.

## Views without allocating

`substring` returns a new `string`. When you only need to look at part of a string (to compare it, parse it, or look it up in a map), take a `StringSpan` instead. `s.span(start, end)` clamps like `substring`, but never allocates:

```dream
let line = "port=8080";
let value = line.span(5, line.length);
System.println(value.parse_int().unwrap_or(0));   // 8080
```

`split_iter(sep)` and `lines()` walk the pieces as spans, and `Map<string, V>` / `Set<string>` accept a span key. Call `to_string()` when you need an owned copy. See [Spans](../language/spans.md).

## `StringBuilder`

Grow a string without a new allocation on every `+`:

```dream
let b = StringBuilder();
b.append("hello");
b.append_line(" world");
System.println(b.build());
```

Also: `append_char`, `append_int`, `append_long`, `append_double`, `append_bool`, `.length`, `is_empty()`, `clear()`, `to_string()`. `append_int` writes decimal digits into the builder (no intermediate `to_string()` allocation).

## Join an array of strings

`parts.join(separator)` combines a string array into one string. For example, `["Ada", "Sam"].join(", ")` returns `"Ada, Sam"`. Use [List.join](collections/list.md) when you have a list instead.

## Choose a Unicode normalization form

The same visible text can be represented by different sequences of characters. Normalization gives it a consistent representation before comparison or storage.

| Form | What it does |
| --- | --- |
| `UnicodeNormForm.Nfc` | Combines characters where a standard combined form exists |
| `UnicodeNormForm.Nfd` | Separates combined characters into their standard components |
| `UnicodeNormForm.Nfkc` | Also replaces compatibility forms, such as presentation variants |
| `UnicodeNormForm.Nfkd` | Applies compatibility replacements and separates components |

Use `text.normalize(UnicodeNormForm.Nfc)` or `Unicode.normalize(text, UnicodeNormForm.Nfc)`. Compatibility forms can change distinctions in the original text; choose them deliberately.

`graphemes()` returns the pieces a reader sees as characters. This differs from `.length`, which counts the stored UTF-16 units. Use graphemes when splitting visible text, especially text containing accents or emoji.

For reusable message patterns, read [Formatting](formatting.md). Exact methods and overloads are in the [string reference](../api/text-string.md), [Unicode reference](../api/text-unicode.md), and [StringBuilder reference](../api/core-string-builder.md).

`StringBuilder.append_utf8_slice(text, start, byte_len)` is an advanced method: its positions and length refer to the string's stored payload bytes. Use ordinary `append` for whole strings. An invalid nonempty byte range stops the program.
