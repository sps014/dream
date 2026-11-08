# StringEncoding

**Import:** `import system.interop;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `StringEncoding`

How a `@c` extern exchanges `string` with C.

```dream
public enum StringEncoding
```

## `Utf8`

A null-terminated `const char*` of UTF-8.

```dream
Utf8
```

## `Utf16`

A null-terminated `const uint16_t*` (`char16_t*` / Windows `LPWSTR`, not `wchar_t`).

```dream
Utf16
```
