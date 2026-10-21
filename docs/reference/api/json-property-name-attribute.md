# PropertyNameAttribute

**Import:** `import system.json;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `PropertyNameAttribute`

Overrides the JSON property name of a field of a `@json` type.

```dream
public struct PropertyNameAttribute
```

## `name`

Property name used in JSON for this field.

```dream
public name: string
```
