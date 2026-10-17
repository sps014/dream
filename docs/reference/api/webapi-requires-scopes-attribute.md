# RequiresScopesAttribute

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `RequiresScopesAttribute`

Requires space-separated permission scopes from a verified CurrentUser dependency.

```dream
public struct RequiresScopesAttribute
```

## `scopes`

Space-separated permissions that the current user must have.

```dream
public scopes: string
```
