# ValidateWithAttribute

**Import:** `import system.validation;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `ValidateWithAttribute`

A synchronous field validator returning an optional error message.

```dream
public struct ValidateWithAttribute
```

## `validator`

Synchronous validation function; returns `None` on success or an error message on failure.

```dream
public validator: DeclarationPath
```
