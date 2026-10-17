# MarshalAttribute

**Import:** `import system.interop;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `MarshalAttribute`

`@c` marshaling: on the extern, string encoding (`@marshal(MarshalKind.LpStr)` / `@marshal(MarshalKind.LpWStr)`); on a `NativeCallback` parameter, `@marshal(MarshalKind.UserDataLast)` passes `user_data` after the callback's own arguments.

```dream
public struct MarshalAttribute
```

## `kind`

Category of this entry.

```dream
public kind: MarshalKind
```
