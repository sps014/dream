# CPtr, OwnedCPtr, NativeCallback, Ffi

**Import:** `import system;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `read_cstring`

Copies the NUL-terminated UTF-8 C string at `ptr` into a Dream `string`; `None` for `NULL`.

```dream
public static fun read_cstring(ptr: CPtr): Option<string>
```
