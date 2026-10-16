# RegexFlags

**Import:** `import system.text;`

Read the [usage guide](../stdlib/string.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `RegexFlags`

Compile-time regex options. Combine with `|` (`RegexFlags.Global | RegexFlags.IgnoreCase`).

```dream
public enum RegexFlags
```

## `None = 0`

No optional matching flags are enabled.

```dream
None = 0
```

## `Global = 1`

Match every occurrence rather than only the first.

```dream
Global = 1
```

## `IgnoreCase = 2`

Ignore case when matching.

```dream
IgnoreCase = 2
```

## `Multiline = 4`

Allow line anchors to match individual lines.

```dream
Multiline = 4
```

## `DotAll = 8`

Allow a dot to match newline characters.

```dream
DotAll = 8
```

## `has`

Returns whether this flag set contains `flag`.

```dream
public fun has(flag: RegexFlags): bool
```
