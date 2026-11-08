# Ffi

**Import:** `import system;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Ffi`

Reads and writes foreign memory from `@c` callbacks. An indexed access panics when `base` is null.

```dream
public static class Ffi
```

## `read_byte`

`((uint8_t*)base)[index]`.

```dream
public static fun read_byte(base: CPtr, index: isize): byte
```

## `read_int`

`((int32_t*)base)[index]`.

```dream
public static fun read_int(base: CPtr, index: isize): int
```

## `read_uint`

`((uint32_t*)base)[index]`.

```dream
public static fun read_uint(base: CPtr, index: isize): uint
```

## `read_long`

`((int64_t*)base)[index]`.

```dream
public static fun read_long(base: CPtr, index: isize): long
```

## `read_ulong`

`((uint64_t*)base)[index]`.

```dream
public static fun read_ulong(base: CPtr, index: isize): ulong
```

## `read_float`

`((float*)base)[index]`.

```dream
public static fun read_float(base: CPtr, index: isize): float
```

## `read_double`

`((double*)base)[index]`.

```dream
public static fun read_double(base: CPtr, index: isize): double
```

## `read_isize`

`((ptrdiff_t*)base)[index]`.

```dream
public static fun read_isize(base: CPtr, index: isize): isize
```

## `read_usize`

`((size_t*)base)[index]`.

```dream
public static fun read_usize(base: CPtr, index: isize): usize
```

## `read_ptr`

`((void**)base)[index]`.

```dream
public static fun read_ptr(base: CPtr, index: isize): CPtr
```

## `write_byte`

`((uint8_t*)base)[index] = value`.

```dream
public static fun write_byte(base: CPtr, index: isize, value: byte): void
```

## `write_int`

`((int32_t*)base)[index] = value`.

```dream
public static fun write_int(base: CPtr, index: isize, value: int): void
```

## `write_uint`

`((uint32_t*)base)[index] = value`.

```dream
public static fun write_uint(base: CPtr, index: isize, value: uint): void
```

## `write_long`

`((int64_t*)base)[index] = value`.

```dream
public static fun write_long(base: CPtr, index: isize, value: long): void
```

## `write_ulong`

`((uint64_t*)base)[index] = value`.

```dream
public static fun write_ulong(base: CPtr, index: isize, value: ulong): void
```

## `write_float`

`((float*)base)[index] = value`.

```dream
public static fun write_float(base: CPtr, index: isize, value: float): void
```

## `write_double`

`((double*)base)[index] = value`.

```dream
public static fun write_double(base: CPtr, index: isize, value: double): void
```

## `write_isize`

`((ptrdiff_t*)base)[index] = value`.

```dream
public static fun write_isize(base: CPtr, index: isize, value: isize): void
```

## `write_usize`

`((size_t*)base)[index] = value`.

```dream
public static fun write_usize(base: CPtr, index: isize, value: usize): void
```

## `write_ptr`

`((void**)base)[index] = value`.

```dream
public static fun write_ptr(base: CPtr, index: isize, value: CPtr): void
```

## `read_bytes`

Copies `len` bytes starting at `ptr`. A null pointer or a negative length panics.

```dream
public static fun read_bytes(ptr: CPtr, len: isize): byte[]
```

## `read_cstring`

Copies the NUL-terminated UTF-8 C string at `ptr` into a Dream `string`; `None` for `NULL`.

```dream
public static fun read_cstring(ptr: CPtr): Option<string>
```

## `read_cstring`

Copies at most `max` bytes of the UTF-8 C string at `ptr`, stopping at the first NUL. `None` for `NULL`.

```dream
public static fun read_cstring(ptr: CPtr, max: isize): Option<string>
```
