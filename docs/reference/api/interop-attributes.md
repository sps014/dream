# system.interop: Attribute declarations

Import `system.interop` to use these declarations.

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

## `CAttribute`

Binds an extern function to a C ABI library/symbol: `@c("lib", "symbol")`. `@c("lib")` uses the Dream name as the symbol; bare `@c` also binds to the declaring package's `native/` sources. Native and portable WASM builds are supported; runtime annotations restrict availability.

```dream
public struct CAttribute
```

## `library`

Optional native library containing the foreign symbol.

```dream
public library: Option<string>
```

## `symbol`

Identifier of the referenced symbol.

```dream
public symbol: Option<string>
```

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

## `PackedAttribute`

Packs a value struct with no padding for C ABI layout (`@packed`).

```dream
public struct PackedAttribute
```

## `CppAttribute`

Binds a class, unmanaged struct, or external function to a C++ declaration. Supply a header relative to the package’s `native/include/` and an optional qualified C++ name.

```dream
public struct CppAttribute
```

## `header`

Header file declaring the foreign symbol.

```dream
public header: string
```

## `name`

Name identifying this entry.

```dream
public name: Option<string>
```

## `CppNameAttribute`

The C++ member (or expression) a `@cpp` extern calls, for renames and template instantiations: `@cpp_name("get_as<int>")`.

```dream
public struct CppNameAttribute
```

## `name`

Name identifying this entry.

```dream
public name: string
```
