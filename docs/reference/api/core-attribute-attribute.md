# AttributeAttribute

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AttributeAttribute`

Declares a `@name(...)` attribute type. Arguments are `AttributeTarget.*` placements (none = anywhere); fields are the argument schema.

```dream
public struct AttributeAttribute
```

## `targets`

Declaration kinds to which this attribute may be applied.

```dream
public targets: AttributeTarget[]
```
