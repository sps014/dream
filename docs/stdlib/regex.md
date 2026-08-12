# Regex

**Package:** `system.text` — `import system.text;`

Pure-Dream regex engine (no host `RegExp`). Synchronous on every host.

```dream
import system;
import system.text;

fun main(): void {
    let digits = Regex("\\d+", RegexFlags.Global);
    System.println(digits.test("abc123"));           // true
    System.println(digits.replace("a1b2c3", "#"));   // a#b#c#
    System.println(digits.match("a1b2c3").length);   // 3
}
```

## Flags

`RegexFlags` is a bitflag enum. Combine values with `|`. Omitting the argument is `RegexFlags.None`.

| Flag | Meaning |
| --- | --- |
| `RegexFlags.None` | no options (the default when `flags` is omitted) |
| `RegexFlags.Global` | `replace` all matches; `match` returns every match |
| `RegexFlags.IgnoreCase` | case-insensitive |
| `RegexFlags.Multiline` | `^`/`$` at line boundaries |
| `RegexFlags.DotAll` | `.` matches newlines |

## Methods

#### `Regex(pattern: string, flags: RegexFlags = 0)`

Compiles a pattern with optional `RegexFlags`. Compile once and reuse the `Regex` object — compilation is not free, and each instance keeps a matcher VM. Prefer `test` when you only need presence; global `match` walks every hit.

```dream
let re = Regex("\\w+", RegexFlags.Global | RegexFlags.IgnoreCase);
```

#### `test(input: string): bool`

Returns whether the pattern matches anywhere in `input`. Fast yes/no check — use before `match` when you only need presence.

```dream
System.println(Regex("\\d+").test("abc123"));  // true
```

#### `replace(input: string, replacement: string): string`

Returns `input` with matches replaced by `replacement`. With `RegexFlags.Global`, replaces every match; supports `$1`..`$9`, `$&`, and `$$` in the replacement string.

```dream
System.println(Regex("(\\d+)", RegexFlags.Global).replace("a1b2", "[$1]"));  // a[1]b[2]
```

#### `match(input: string): string[]`

Returns match results as a string array. With `RegexFlags.Global`: every full match; without it: full match plus capture groups (`""` for non-participating groups).

```dream
let all = Regex("\\d+", RegexFlags.Global).match("a1b2c3");       // ["1", "2", "3"]
let caps = Regex("(\\d{4})-(\\d{2})").match("2026-06");
System.println(caps[1]);  // 2026
```

#### `match_info(input: string): Option<RegexMatchInfo>`

Richer match result for the first match: `.full` (the overall match text), `.groups` (each capture group's text in group-number order — `groups[0]` is group 1, since `.full` already covers group 0; `""` for a group that didn't participate), and `.named(name)` to look a group up by the name given via `(?<name>...)`. Prefer this over `match` when you need named-group lookup or want to distinguish "no match" from an empty capture.

```dream
let re = Regex("(?<year>\\d{4})-(?<month>\\d{2})");
switch (re.match_info("2026-06")) {
    Some(info) => {
        System.println(info.full);                      // 2026-06
        System.println(info.groups[0]);                  // 2026
        System.println(info.named("month").unwrap_or("")); // 06
    },
    None => System.println("no match"),
}
```

## Supported syntax

| Feature | Syntax |
| --- | --- |
| Literals, any character | `a`, `.` (respects `s`) |
| Anchors | `^`, `$` (respect `m`), `\b`, `\B` |
| Quantifiers | `*`, `+`, `?`, `{m}`, `{m,}`, `{m,n}`, lazy forms |
| Alternation | `a\|b` |
| Groups | `(...)`, `(?:...)`, named `(?<name>...)` |
| Lookaround | `(?=...)`, `(?!...)`, `(?<=...)`, `(?<!...)` |
| Backreferences | `\1`..`\9`, `\k<name>` |
| Character classes | `[abc]`, `[^abc]`, `[a-z]` |
| Shorthands | `\d`, `\D`, `\w`, `\W`, `\s`, `\S` |
| Unicode property | `\p{L}`/`\p{Letter}`, `\p{N}`/`\p{Number}`, and their negations `\P{...}` |
| Escapes | `\n`, `\t`, `\r`, `\` before metacharacters |

Patterns without a backreference run on Dream's Pike-VM (linear time, including lookaround via
compiled sub-programs); a pattern containing a backreference falls back to a backtracking
interpreter, since backreferences aren't a regular-language construct.

`\p{...}`/`\P{...}` only recognizes the `L` (Letter) and `N` (Number) categories, and is
range-based rather than a full Unicode Character Database — it covers the common scripts/ranges
for each category, not every codepoint the Unicode Standard assigns to it. Other `\p{...}` names
parse best-effort without special meaning.

A runnable example: [`sample/interop/regex.dream`](https://github.com/sps014/dream/blob/main/sample/interop/regex.dream).
