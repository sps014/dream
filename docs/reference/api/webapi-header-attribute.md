# HeaderAttribute

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `HeaderAttribute`

Binds a header value, with an optional name override.

```dream
public struct HeaderAttribute
```

## `name`

Optional input name; omitted means use the parameter name.

```dream
public name: Option<string>
```
