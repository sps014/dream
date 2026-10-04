# Fmt

**Import:** `import system.text;`

Read the [usage guide](../stdlib/formatting.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Fmt`

`{{` / `}}` escape literal braces.

```dream
public static class Fmt
```

## `format`

Substitutes every `{i}`/`{i:spec}` placeholder in `pattern` with `args[i]`.

```dream
public static fun format(pattern: string, args: object[]): string
```
