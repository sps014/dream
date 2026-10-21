# CppAttribute

**Import:** `import system.interop;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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

Qualified C++ name to bind, when it differs from the Dream name.

```dream
public name: Option<string>
```
