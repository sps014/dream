# RegexFlags

**Import:** `import system.text;`

Read the [usage guide](../stdlib/string.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `enum RegexFlags`

Compile-time regex options. Combine with ` / ` (`RegexFlags.Global  /  RegexFlags.IgnoreCase`).

```dream
public enum RegexFlags
```

## `None = 0`

```dream
None = 0
```

## `Global = 1`

```dream
Global = 1
```

## `IgnoreCase = 2`

```dream
IgnoreCase = 2
```

## `Multiline = 4`

```dream
Multiline = 4
```

## `DotAll = 8`

```dream
DotAll = 8
```

## `has`

```dream
public fun has(flag: RegexFlags): bool
```
