# Strings

`string` is UTF-8 text. `+` and `$"hi {name}"` need no import. Extra helpers: `import system.text;` (also pulled in by `import system;`).

```dream
import system;

fun main() {
    let name = "Ada";
    System.println("Hello, " + name);
    System.println($"hi {name}");
    System.println("hello".to_upper());
}
```

`.length` counts Unicode scalars, not bytes. Use `.byte_size()` for UTF-8 size. `string.empty` is the shared empty string.

## Look up and change

| Call | Meaning |
| --- | --- |
| `char_at(i)` / `s[i]` / `get(i)` | character at index |
| `s[i] = c` / `set_at(i, c)` | replace a character |
| `byte_at(i)` | one UTF-8 byte |
| `for (let c in s)` | walk characters |
| `string.alloc(n)` | allocate `n` scalars (low-level) |

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

Also: `append_char`, `.length`, `is_empty()`, `clear()`, `to_string()`.
