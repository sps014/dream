# UserDataAttribute

**Import:** `import system.interop;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `UserDataAttribute`

Where a `NativeCallback` parameter receives `user_data`. The default is `UserDataPosition.First`.

```dream
public struct UserDataAttribute
```

## `position`

Position of `user_data` among the C arguments.

```dream
public position: UserDataPosition
```
