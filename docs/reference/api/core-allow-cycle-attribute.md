# AllowCycleAttribute

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AllowCycleAttribute`

Accepts strong reference cycles that pass through objects of this class. The compiler otherwise rejects stores that close a strong cycle. ARC does not collect cycles, so the program must break them itself, for example by clearing an owning field.

```dream
public struct AllowCycleAttribute
```
