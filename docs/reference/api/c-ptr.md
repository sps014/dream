# CPtr

**Import:** `import system;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `CPtr`

An opaque C pointer (`void*`, `T*`). `@c` passes it by value; `ref p: CPtr` is a `T**` out-param and `Option<CPtr>` a nullable pointer.

```dream
public struct CPtr
```

## `constructor`

Creates a `CPtr` with `raw`.

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
