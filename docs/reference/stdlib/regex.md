# Regex

**Import:** `import system.text;`

A regex engine written in Dream (not the host `RegExp`). Works the same on every host.

```dream
import system;
import system.text;

fun main() {
    let digits = Regex("\\d+", RegexFlags.Global);
    System.println(digits.test("abc123"));
    System.println(digits.replace("a1b2c3", "#"));
}
```

## Flags

Combine with `|`. Default is `RegexFlags.None`.

| Flag | Meaning |
| --- | --- |
| `Global` | replace / match every occurrence |
| `IgnoreCase` | case-insensitive |
| `Multiline` | `^` / `$` at line edges |
| `DotAll` | `.` matches newlines |

## Methods

| Call | Meaning |
| --- | --- |
| `Regex(pattern, flags)` | compile |
| `test(input)` | any match? |
| `replace(input, replacement)` | substitute |
| `match(input)` | `string[]` of matches |
| `match_info(input)` | `Option<RegexMatchInfo>` with groups and indices |

A full example: [`sample/interop/regex.dream`](https://github.com/sps014/dream/blob/main/sample/interop/regex.dream).
