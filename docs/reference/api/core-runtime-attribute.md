# RuntimeAttribute

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `RuntimeAttribute`

Binds an external function to a named Dream host service on supported platforms.

```dream
public struct RuntimeAttribute
```

## `export_name`

Name exposed to callers of the foreign function.

```dream
public export_name: string
```
