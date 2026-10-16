# Regex

**Import:** `import system.text;`

Read the [usage guide](../stdlib/string.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Regex`

A compiled regular expression. An invalid pattern never matches.

```dream
public class Regex
```

## `constructor`

Compiles `pattern` with optional matching flags. Invalid patterns never match.

```dream
public constructor(pattern: string, flags: RegexFlags = 0)
```

## `test`

Returns whether the regular expression matches anywhere in `input`.

```dream
public fun test(borrow input: string): bool
```

## `replace`

Replaces matching text. Replaces every match with `Global`; otherwise replaces only the first.

```dream
public fun replace(input: string, replacement: string): string
```

## `match`

Returns complete matches with `Global`, or the first match followed by its capture groups otherwise. Returns an empty array when unmatched.

```dream
public fun match(input: string): string[]
```

## `match_info`

Returns the first match and its named and numbered capture groups, or `None` when unmatched.

```dream
public fun match_info(input: string): Option<RegexMatchInfo>
```
