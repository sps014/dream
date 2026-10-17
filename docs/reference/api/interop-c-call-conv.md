# CCallConv

**Import:** `import system.interop;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `CCallConv`

Calling convention for C imports.

```dream
public enum CCallConv
```

## `Cdecl`

The platform default C calling convention.

```dream
Cdecl
```

## `Stdcall`

The stdcall convention on 32-bit Windows; equivalent to the default convention on other targets.

```dream
Stdcall
```
