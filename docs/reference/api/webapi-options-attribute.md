# OptionsAttribute

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `OptionsAttribute`

Declares an HTTP OPTIONS route.

```dream
public struct OptionsAttribute
```

## `path`

Route path relative to the router prefix; named segments use `{name}`.

```dream
public path: string
```
