# system.interop: Attribute declarations

Import `system.interop` to use these declarations.

```dream
public enum CCallConv
```

Calling convention for C imports.

```dream
public enum MarshalKind
```

C string encoding or callback context placement.

```dream
public struct CAttribute
```

Binds an extern function to a C ABI library/symbol: `@c("lib", "symbol")`. `@c("lib")` uses the Dream name as the symbol; bare `@c` also binds to the declaring package's `native/` sources. Native and portable WASM builds are supported; runtime annotations restrict availability.

```dream
public library: Option<string>
```

```dream
public symbol: Option<string>
```

```dream
public struct CCallAttribute
```

C calling convention for `@c` externs: `@c_call(CCallConv.Cdecl)` (the platform default) or `@c_call(CCallConv.Stdcall)` (Win32 APIs; identical to cdecl on every target but 32-bit x86 Windows).

```dream
public convention: CCallConv
```

```dream
public struct MarshalAttribute
```

`@c` marshaling: on the extern, string encoding (`@marshal(MarshalKind.LpStr)` / `@marshal(MarshalKind.LpWStr)`); on a `NativeCallback` parameter, `@marshal(MarshalKind.UserDataLast)` passes `user_data` after the callback's own arguments.

```dream
public kind: MarshalKind
```

```dream
public struct PackedAttribute
```

Packs a value struct with no padding for C ABI layout (`@packed`).

```dream
public struct CppAttribute
```

Binds a class, `@unmanaged` struct, or free extern function to C++: `@cpp("header.hpp", "ns::Name")`. The header resolves against the declaring package's `native/include/`; the name defaults to the Dream name. The compiler generates the `extern "C"` shim for native or portable WASM builds.

```dream
public header: string
```

```dream
public name: Option<string>
```

```dream
public struct CppNameAttribute
```

The C++ member (or expression) a `@cpp` extern calls, for renames and template instantiations: `@cpp_name("get_as<int>")`.

```dream
public name: string
```
