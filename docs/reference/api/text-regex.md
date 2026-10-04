# Regex

**Import:** `import system.text;`

Read the [usage guide](../stdlib/string.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class Regex`

```dream
public class Regex
```

## `constructor`

```dream
public constructor(pattern: string, flags: RegexFlags = 0)
```

## `test`

```dream
public fun test(input: string): bool
```

## `replace`

```dream
public fun replace(input: string, replacement: string): string
```

## `match`

```dream
public fun match(input: string): string[]
```

## `match_info`

```dream
public fun match_info(input: string): Option<RegexMatchInfo>
```
