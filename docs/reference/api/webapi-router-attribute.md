# RouterAttribute

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `RouterAttribute`

Declares an explicitly mounted group of static HTTP handlers.

```dream
public struct RouterAttribute
```

## `prefix`

Path prefix applied to every route in the group.

```dream
public prefix: string
```
