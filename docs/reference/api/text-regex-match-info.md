# RegexMatchInfo

**Import:** `import system.text;`

Read the [usage guide](../../library/text/string.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `RegexMatchInfo`

Rich match result returned by `Regex.match_info`. `full` is the overall match text; `groups` holds each capturing group's text in group-number order (1, 2, 3, ... — `groups[0]` is group 1, since `full` already covers group 0), with `""` for a group that didn't participate; `named` looks a group up by the name it was given via `(?<name>...)`.

```dream
public class RegexMatchInfo
```

## `full`

Text of the complete regular-expression match.

```dream
public full: string
```

## `groups`

Text captured by each numbered group.

```dream
public groups: string[]
```

## `named`

The text captured by the named group `name`, or `None` if no such name exists on this pattern or the group didn't participate in the match.

```dream
public fun named(name: string): Option<string>
```
