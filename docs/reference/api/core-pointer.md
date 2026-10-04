# Pointer

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct Pointer<T>`

Pointer gives manual control over an array's storage. Lifetime-changing operations require `@unsafe`; you must avoid freeing it twice or accessing it after it is freed. Use an ordinary array or Span when automatic cleanup meets your needs. Read [raw buffers](../language/memory-buffers.md) before choosing manual storage.

```dream
public struct Pointer<T>
```

## `constructor`

Wraps an existing block; prefer `alloc` for a fresh pointer.

```dream
public constructor(buf: T[])
```

## `alloc`

Allocates a fresh, zero-initialized block of `count` elements.

```dream
public static fun alloc(count: int): Pointer<T>
```

## `length`

Number of elements currently addressable through this pointer.

```dream
public get length(): int
```

## `this`

Bounds-checked read (stops the program if `index` is out of range, exactly like `T[]` indexing).

```dream
public fun this[index: int]: T
```

## `this`

Bounds-checked write (stops the program if `index` is out of range).

```dream
public fun this[index: int] = value: T
```

## `get`

```dream
public fun get(index: int): T
```

## `set`

```dream
public fun set(index: int, value: T): void
```

## `realloc`

Resizes the block in place via `Buffer.realloc`: the overlapping prefix is preserved and any newly grown tail is zero-initialized. After this call, no other alias of the pointer's old block may be read again — the same single-owner contract as `Buffer.realloc` itself.

```dream
public fun realloc(new_count: int): void
```

## `free`

Immediately returns the block to the allocator, bypassing reference counting. Calling `get`/`set`/`realloc`/`free` again on this (or any copy of this) `Pointer<T>` after `free()` is undefined behavior - the compiler cannot check it, by design (`@unsafe`).

```dream
public fun free(): void
```
