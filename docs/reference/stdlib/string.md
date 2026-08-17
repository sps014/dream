# Strings

`string` is UTF-16 text in memory (C#/JS-style code units). Files, JSON on the wire, and `Encoding.utf8_*` still use UTF-8. `+` and `$"hi {name}"` need no import. Extra helpers: `import system.text;` (also pulled in by `import system;`).

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
| `index_of(char or string)` | `Option<int>` |
| `split(sep)` | `string[]` |
| `replace(old, new)` | new string |
| `substring(start, end)` | slice |
| `to_lower` / `to_upper` | ASCII case |
| `to_lower_unicode` / `to_upper_unicode` | full Unicode case |
| `trim` / `repeat(n)` | whitespace / repeat |
| `normalize(form)` / `graphemes()` | Unicode normalize / grapheme clusters |
| `equals` / `compare` | equality / ordering |

`Unicode.normalize`, `Unicode.to_lower_unicode`, `Unicode.to_upper_unicode`, and `Unicode.graphemes` are the static forms of the same helpers.

## `StringBuilder`

Grow a string without a new allocation on every `+`:

```dream
let b = StringBuilder();
b.append("hello");
b.append_line(" world");
System.println(b.build());
```

Also: `append_char`, `append_int`, `append_bool`, `.length`, `is_empty()`, `clear()`, `to_string()`. `append_int` writes decimal digits into the builder (no intermediate `to_string()` allocation).
