# String Abi

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `empty`

The interned empty string. Prefer this over `""` so empty results share one immortal pointer instead of a fresh heap block (substring/from_utf8/concat of empties also return this).

```dream
public static get empty(): string
```
