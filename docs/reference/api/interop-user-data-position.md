# UserDataPosition

**Import:** `import system.interop;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `UserDataPosition`

Where a `NativeCallback` parameter places `user_data` among the C arguments.

```dream
public enum UserDataPosition
```

## `First`

`user_data` is the first argument.

```dream
First
```

## `Last`

`user_data` is the last argument.

```dream
Last
```
