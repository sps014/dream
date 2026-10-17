# JsAttribute

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `JsAttribute`

Binds an external function to a JavaScript export using `@js("module", "export")`. Available only on JavaScript platforms.

```dream
public struct JsAttribute
```

## `module_name`

Fully qualified module containing the declaration.

```dream
public module_name: string
```

## `export_name`

Name exposed to callers of the foreign function.

```dream
public export_name: string
```
