# WebAttribute

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `WebAttribute`

Marks a function/method as available in the browser host. Combine with `@native`/`@node` to restrict; absent all three means every runtime.

```dream
public struct WebAttribute
```
