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

## `free_with`

Calls the supplied C deallocator address with the pointer address. `OwnedCPtr` uses this during destruction.

```dream
public static extern fun free_with(free: usize, ptr: usize): void
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

## `register`

Registers a native callback with its owning Dream thread. Prefer constructing `NativeCallback`.

```dream
public static extern fun register(borrow callback: object): void
```

## `unregister`

Unregisters a native callback. The callback owner manages registration lifetime.

```dream
public static extern fun unregister(borrow callback: object): void
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

## `host_read_ptr`

Reads a pointer-sized value from foreign memory at the element index. The caller must provide valid readable memory.

```dream
public static extern fun host_read_ptr(base: usize, index: int): usize
```

## `host_read_cstring`

Copies a NUL-terminated C string into a Dream string. The pointer must remain readable through the terminator.

```dream
public static extern fun host_read_cstring(ptr: usize): string
```

## `host_read_i32`

Reads a 32-bit integer from foreign memory at the element index.

```dream
public static extern fun host_read_i32(base: usize, index: int): int
```

## `host_read_i64`

Reads a 64-bit integer from foreign memory at the element index.

```dream
public static extern fun host_read_i64(base: usize, index: int): long
```

## `host_read_f64`

Reads a double from foreign memory at the element index.

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
