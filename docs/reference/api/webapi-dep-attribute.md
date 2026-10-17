# DepAttribute

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `DepAttribute`

Injects and memoizes a dependency function once per request.

```dream
public struct DepAttribute
```

## `provider`

Dependency function to evaluate for this parameter or replace for this application.

```dream
public provider: DeclarationPath
```

## `cache`

Whether the dependency result is reused within this request; omitted means true.

```dream
public cache: Option<bool>
```
