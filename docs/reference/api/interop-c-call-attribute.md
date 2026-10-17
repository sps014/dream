# CCallAttribute

**Import:** `import system.interop;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `CCallAttribute`

C calling convention for `@c` externs: `@c_call(CCallConv.Cdecl)` (the platform default) or `@c_call(CCallConv.Stdcall)` (Win32 APIs; identical to cdecl on every target but 32-bit x86 Windows).

```dream
public struct CCallAttribute
```

## `convention`

Calling convention used by the foreign function.

```dream
public convention: CCallConv
```
