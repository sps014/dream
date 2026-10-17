# ConsumingAttribute

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `ConsumingAttribute`

Marks an extern whose reference parameters transfer ownership to the host, which must release them. Extern parameters are otherwise passed borrowed, since a host cannot release a Dream reference.

```dream
public struct ConsumingAttribute
```
