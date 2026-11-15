---
title: Strings
summary: "Join, interpolate, search, and change text."
icon: material/format-quote-close
api: reference/api/system-text.md
---

# Strings

Join text with `+`, interpolation, or the helpers below. Stored text is UTF-16, so use graphemes when you need visible characters.

```dream
import system;

fun main() {
    let name = "Ada";
    System.println("Hello, " + name);
    System.println($"hi {name}");
    System.println("hello".to_upper());
}
```

`.length` counts UTF-16 code units, not Unicode scalars. `.byte_size()` is that storage in bytes (`length * 2`).

## Raw strings

A raw string starts and ends with three or more double quotes. Backslashes and `"` are ordinary characters, which suits paths, regular expressions, and JSON.

```dream
let path = """C:\temp\new""";
let pattern = """^\d+(\.\d+)?$""";
let quoted = """She said "hi".""";
```

`path` holds `C:\temp\new`; `\t` and `\n` stay two characters each. A raw string cannot start or end with `"` on the same line as its quotes, so add a space or use the multi-line form for that text.

To put three quotes in the text, open and close with more quotes than any run inside: `""""a """ inside""""` holds `a """ inside`.

### Multi-line raw strings

Start the text on the line after the opening quotes and put the closing quotes on their own line. The whitespace before the closing quotes is removed from every line, so the literal can follow the indentation of your code:

```dream
fun usage(): string {
    return """
        Usage: tool [options]
          --verbose  print each step
        """;
}
```

The result is `"Usage: tool [options]\n  --verbose  print each step"`. The line break after the opening quotes and the one before the closing quotes are not part of the text, and blank lines stay blank.

The compiler reports an error when text follows the opening quotes on the same line, when text comes before the closing quotes on their line, or when a nonblank line does not begin with the closing line's whitespace.

### Interpolated raw strings

Put `$` before the quotes to insert values with `{expression}`, like an ordinary interpolated string. Write `{{` and `}}` for literal braces:

```dream
fun user_json(name: string, age: int): string {
    return $"""
        {{
          "name": "{name}",
          "age": {age}
        }}
        """;
}
```

`user_json("Ada", 36)` returns a three-line JSON object. Values are inserted as text; escape them yourself when the surrounding format needs it, or build JSON with [`Json.serialize`](../serialization/json.md).

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

These helpers follow Unicode 16.0 and match on every target, including the browser. Case mapping ignores locale: `"İ".to_lower_unicode()` is `"i̇"`, a final capital sigma becomes `ς`, and `"straße".to_upper_unicode()` is `"STRASSE"`.

## Views without allocating

`substring` returns a new `string`. When you only need to look at part of a string (to compare it, parse it, or look it up in a map), take a `ReadOnlySpan<char>` instead. `s.span(start, end)` clamps like `substring`, but never allocates:

```dream
let line = "port=8080";
let value = line.span(5, line.length);
System.println(value.parse_int().unwrap_or(0)); // 8080
```

Compare a span with a string using `sp == "text"` or `"text" == sp`; `!=` works in both orders too. These compare contents without allocating.

`split_iter(sep)` and `lines()` yield spans, and `Map<string, V>` and `Set<string>` accept a span key. Call `to_string()` for an owned copy ([Spans](../../language/types/spans.md)).

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

`parts.join(separator)` joins a string array, so `["Ada", "Sam"].join(", ")` is `"Ada, Sam"`. Use [List.join](../collections/list.md) for a list.

## Choose a Unicode normalization form

The same visible text can be represented by different sequences of characters. Normalization gives it a consistent representation before comparison or storage.

| Form | What it does |
| --- | --- |
| `UnicodeNormForm.Nfc` | Combines characters where a standard combined form exists |
| `UnicodeNormForm.Nfd` | Separates combined characters into their standard components |
| `UnicodeNormForm.Nfkc` | Also replaces compatibility forms, such as presentation variants |
| `UnicodeNormForm.Nfkd` | Applies compatibility replacements and separates components |

Use `text.normalize(UnicodeNormForm.Nfc)`. Compatibility forms can change distinctions in the original text; choose them deliberately.

`graphemes()` returns the pieces a reader sees as characters, unlike `.length`, which counts UTF-16 units. Use it to split visible text, including accents and emoji.

For reusable message patterns, read [Formatting](formatting.md). Exact methods and overloads are in the [string reference](../../reference/api/text-string-text.md), [normalization forms reference](../../reference/api/text-unicode-norm-form.md), and [StringBuilder reference](../../reference/api/core-string-builder.md).

`append_utf8_slice(text, start, byte_len)` indexes stored bytes, not characters. Use `append` for a whole string; a bad nonempty range stops the program.
