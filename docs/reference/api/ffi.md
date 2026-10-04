# CPtr, OwnedCPtr, NativeCallback, Ffi

**Import:** `import system;`

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct CPtr`

An opaque C pointer (`void*`, `T*`). `@c` passes it by value; `ref p: CPtr` is a `T**` out-param and `Option<CPtr>` a nullable pointer.

```dream
public struct CPtr
```

## `constructor`

```dream
public constructor(raw: usize)
```

## `null`

The `NULL` pointer.

```dream
public static fun null(): CPtr
```

## `is_null`

True when this is `NULL`.

```dream
public fun is_null(): bool
```

## `address`

The pointer as an integer address.

```dream
public fun address(): usize
```

## `offset`

The pointer `bytes` bytes past this one.

```dream
public fun offset(bytes: isize): CPtr
```

## `class OwnedCPtr`

A C pointer returned by an `@owned("free_fn")` `@c` extern: Dream owns it, and the destructor calls `free_fn(ptr)` unless `take()` handed ownership back first.

```dream
public class OwnedCPtr
```

## `get`

The pointer, still owned (and later freed) by this object.

```dream
public fun get(): CPtr
```

## `take`

Gives up ownership: the caller now frees the returned pointer, and this object holds `NULL`.

```dream
public fun take(): CPtr
```

## `class NativeCallback<F>`

A Dream `fun` (capturing or not) handed to a `@c` extern as a `(fn, void* user_data)` pair. C may call it for as long as this object is alive: keep it in a field while C holds the pointer. Calls from threads Dream did not start trap.

```dream
public class NativeCallback<F>
```

## `constructor`

```dream
public constructor(f: F)
```

## `static class Ffi`

Reads through C pointers from `@c` callbacks (`char**`, `char*`).

```dream
public static class Ffi
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
