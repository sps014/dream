# MarshalKind

**Import:** `import system.interop;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `MarshalKind`

C string encoding or callback context placement.

```dream
public enum MarshalKind
```

## `LpStr`

Marshal text as a null-terminated narrow C string.

```dream
LpStr
```

## `LpWStr`

Marshal text as a null-terminated wide C string.

```dream
LpWStr
```

## `UserDataLast`

Pass the callback context after the callback arguments.

```dream
UserDataLast
```
