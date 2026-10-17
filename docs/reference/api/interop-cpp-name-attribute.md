# CppNameAttribute

**Import:** `import system.interop;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
