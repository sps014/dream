# AllowCycleAttribute

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AllowCycleAttribute`

Acknowledges that this class can form strong ownership cycles. ARC does not collect cycles; use weak/unowned back-links or explicit teardown to avoid leaks.

```dream
public struct AllowCycleAttribute
```
