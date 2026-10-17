# CAttribute

**Import:** `import system.interop;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
