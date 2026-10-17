# Ffi

**Import:** `import system;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Ffi`

Reads through C pointers from `@c` callbacks (`char**`, `char*`).

```dream
public static class Ffi
```

## `host_read_ptr`

Reads the pointer at `index` from a valid foreign pointer array.

```dream
public static extern fun host_read_ptr(base: usize, index: int): usize
```

## `host_read_cstring`

Copies a valid null-terminated foreign string into a Dream string.

```dream
public static extern fun host_read_cstring(ptr: usize): string
```

## `host_read_i32`

Reads the 32-bit integer at `index` from valid foreign storage.

```dream
public static extern fun host_read_i32(base: usize, index: int): int
```

## `host_read_i64`

Reads the 64-bit integer at `index` from valid foreign storage.

```dream
public static extern fun host_read_i64(base: usize, index: int): long
```

## `host_read_f64`

Reads the double at `index` from valid foreign storage.

```dream
public static extern fun host_read_f64(base: usize, index: int): double
```

## `read_int`

`((int32_t*)base)[index]`.

```dream
public static fun read_int(base: CPtr, index: int): int
```

## `read_long`

`((int64_t*)base)[index]`.

```dream
public static fun read_long(base: CPtr, index: int): long
```

## `read_double`

`((double*)base)[index]`.

```dream
public static fun read_double(base: CPtr, index: int): double
```

## `read_ptr`

`((void**)base)[index]`. A `NULL` base reads `NULL`.

```dream
public static fun read_ptr(base: CPtr, index: int): CPtr
```

## `read_cstring`

Copies the NUL-terminated UTF-8 C string at `ptr` into a Dream `string`; `None` for `NULL`.

```dream
public static fun read_cstring(ptr: CPtr): Option<string>
```
