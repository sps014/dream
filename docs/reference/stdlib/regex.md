# Regex

**Import:** `import system.text;`

Compile a pattern once, then test, replace, or extract matches. Patterns use the usual Perl-style syntax (`\d`, groups, lookaround). An invalid pattern never matches.

```dream
import system;
import system.text;

fun main() {
    let digits = Regex("\\d+", RegexFlags.Global);
    System.println(digits.test("abc123"));
    System.println(digits.replace("a1b2c3", "#"));
}
```

```
true
a#b#c#
```

## Flags

Combine with `|`. Default is `RegexFlags.None`.

| Flag | Meaning |
| --- | --- |
| `Global` | replace / match every occurrence |
| `IgnoreCase` | case-insensitive |
| `Multiline` | `^` / `$` at line edges |
| `DotAll` | `.` matches newlines |

```dream
let re = Regex("hello", RegexFlags.IgnoreCase | RegexFlags.Global);
```

## Methods

| Call | Meaning |
| --- | --- |
| `Regex(pattern)` / `Regex(pattern, flags)` | compile |
| `test(input)` | any match? |
| `replace(input, replacement)` | substitute (`$&`, `$1`, `${name}`, `$$`) |
| `match(input)` | `string[]` of matches |
| `match_info(input)` | `Option<RegexMatchInfo>` with groups and names |

`Global` changes `match`: without it, the array is the whole match plus each capturing group; with it, every full match is one element.

```dream
let digits = Regex("\\d+", RegexFlags.Global);
let parts = digits.match("a1b2c3");   // ["1", "2", "3"]
```

## Groups

`match_info` returns the first match, or `None`. `full` is the whole match. `groups` is each capturing group in order (`groups[0]` is group 1). Named groups `(?<name>...)` are looked up with `named`:

```dream
let re = Regex("(?<area>\\d{3})-(?<num>\\d{4})");
switch (re.match_info("555-1234")) {
    Some(info) => {
        System.println(info.full);                    // 555-1234
        System.println(info.named("area").unwrap());  // 555
    }
    None => System.println("no match"),
}
```

A full example: [`sample/interop/regex.dream`](https://github.com/sps014/dream/blob/main/sample/interop/regex.dream).
