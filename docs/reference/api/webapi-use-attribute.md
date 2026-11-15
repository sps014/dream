# UseAttribute

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `UseAttribute`

Applies a middleware function to a route or group in source order.

```dream
public struct UseAttribute
```

## `handler`

Middleware function to run for the annotated route or group.

```dream
public handler: DeclarationPath
```
