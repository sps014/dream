---
title: Regex
summary: "Find, extract, and replace text with regular expressions."
icon: material/regex
api: reference/api/system-text.md
---

# Regex


Use a regular expression to find text matching a pattern. Create it once, then test text, extract matches, or replace them. `Regex.compile` returns an error when the pattern is invalid.

**Import:** `import system.text;`

Compile a pattern once, then test, replace, or extract matches. Patterns use familiar regex syntax (`\d`, groups, lookaround). `Regex.compile` returns `Result<Regex, RegexError>`.

Match positions use UTF-16 code units. After an empty match, the next search advances
past a complete character, including a surrogate pair, so a global search always progresses.

```dream
import system;
import system.text;

fun main() {
    let digits = Regex.compile("\\d+", RegexFlags.Global).unwrap();
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
let re = Regex.compile("hello", RegexFlags.IgnoreCase | RegexFlags.Global).unwrap();
```

## Methods

| Call | Meaning |
| --- | --- |
| `Regex.compile(pattern).unwrap()` / `Regex.compile(pattern, flags).unwrap()` | compile |
| `test(input)` | any match? |
| `replace(input, replacement)` | substitute (`$&`, `$1`, `${name}`, `$$`) |
| `match(input)` | `string[]` of matches |
| `match_info(input)` | `Option<RegexMatchInfo>` with groups and names |

`Global` changes `match`: without it, the array is the whole match plus each capturing group; with it, every full match is one element.

```dream
let digits = Regex.compile("\\d+", RegexFlags.Global).unwrap();
let parts = digits.match("a1b2c3"); // ["1", "2", "3"]
```

## Groups

`match_info` returns the first match, or `None`. `full` is the whole match. `groups` is each capturing group in order (`groups[0]` is group 1). Named groups `(?<name>...)` are looked up with `named`:

```dream
let re = Regex.compile("(?<area>\\d{3})-(?<num>\\d{4})").unwrap();
switch (re.match_info("555-1234")) {
    Some(info) => {
        System.println(info.full); // 555-1234
        System.println(info.named("area").unwrap()); // 555
    }
    None => System.println("no match"),
}
```

Save the first example as `regex.dream` and run `dream run regex.dream`. It prints whether a match exists, then the text with every numeric group replaced by `#`.

## Invalid patterns

Handle patterns supplied by users without unwrapping the result:

```dream
import system;
import system.text;

fun main(): void {
    switch (Regex.compile("[unfinished")) {
        Ok(regex) => System.println(regex.test("example")),
        Err(error) => System.println(error.message()),
    }
}
```

Compilation failures have code `EREGEX`. Successfully compiled patterns that find no match
return `false` from `test` or `None` from `match_info`.
