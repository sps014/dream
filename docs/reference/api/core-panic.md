# System

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class System`

Halt used by bootstrap (`Option.unwrap`, bounds checks). Full console I/O lives on `extend System` in `system.dream` (`import system;`).

```dream
public static class System
```
