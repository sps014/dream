# OwnedAttribute

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `OwnedAttribute`

The extern's result transfers ownership to Dream. On a `@cpp` member returning `T*`, the Dream object deletes it. On a `@c` extern returning `OwnedCPtr`, `@owned("free_fn")` names the C function that frees the pointer when the last reference is released.

```dream
public struct OwnedAttribute
```

## `free_function`

Optional function used to release foreign storage.

```dream
public free_function: Option<string>
```
