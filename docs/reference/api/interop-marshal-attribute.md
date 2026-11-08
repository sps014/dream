# MarshalAttribute

**Import:** `import system.interop;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `MarshalAttribute`

String encoding of a `@c` extern: `@marshal(StringEncoding.Utf8)` or `@marshal(StringEncoding.Utf16)`. It covers string parameters, the string result, and string parameters of callbacks.

```dream
public struct MarshalAttribute
```

## `encoding`

Encoding used for every `string` that crosses this extern.

```dream
public encoding: StringEncoding
```
