# AsyncHostAttribute

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AsyncHostAttribute`

Deferred native host for an `extern async fun`: the `<host>Async` C symbol takes the future as its leading argument, performs the work off-thread, and completes it via the bound dream_complete_foreign. The run loop stays parked while the work is in flight.

```dream
public struct AsyncHostAttribute
```
