# RawPtr

No import is needed.

A non-owning address for unmanaged values. Copies never retain storage. Allocation is uninitialized unless alloc_zeroed is requested. Initialize values before reading. Raw operations require @unsafe and do not check bounds. Only base pointers returned by RawPtr allocation methods may be freed or resized. Foreign allocations require their original deallocator. Reallocation returns a new handle and invalidates every old alias. Null and zero-byte allocations carry address zero; freeing null is a no-op.

[Raw memory guide](../../language/types/arrays.md#rawptrt-raw-memory-unsafe).

## `RawPtr`

An address carries no owning token or length. Reads require initialized storage of T; copies remain valid only as long as the underlying allocation does.

```dream
public struct RawPtr<T: unmanaged>
```

## `null`

Returns a null pointer with address zero.

```dream
public static fun null(): RawPtr<T>
```

## `is_null`

Returns whether the pointer address is zero.

```dream
public fun is_null(): bool
```

## `address`

Returns the numeric address without dereferencing it.

```dream
public fun address(): usize
```

## `operator`

Returns whether both pointers have the same numeric address.

```dream
public fun operator ==(other: RawPtr<T>): bool
```

## `from_address`

Creates a pointer from an address. The caller must ensure alignment, validity, and lifetime before dereferencing.

```dream
public static fun from_address(address: usize): RawPtr<T>
```

## `cast`

Reinterprets the address as `U`; the caller must ensure the new alignment and layout are valid.

```dream
public fun cast<U: unmanaged>(): RawPtr<U>
```

## `offset`

Moves by `elements` values of `T`. The caller must keep the resulting pointer within valid storage before dereferencing.

```dream
public fun offset(elements: isize): RawPtr<T>
```

## `byte_offset`

Moves the address by `bytes`. The caller must ensure alignment and valid storage before dereferencing.

```dream
public fun byte_offset(bytes: isize): RawPtr<T>
```

## `read`

Reads an initialized `T` from the address. The address must be valid and aligned.

```dream
public fun read(): T
```

## `write`

Writes `value` to the address. The destination must be valid, writable, and aligned for `T`.

```dream
public fun write(value: T): void
```

## `this`

Accesses the value at element offset `index`. The caller must ensure valid, aligned storage and initialize it before reading.

```dream
public fun this[index: isize]: T
```

## `this`

Accesses the value at element offset `index`. The caller must ensure valid, aligned storage and initialize it before reading.

```dream
public fun this[index: isize] = value: T
```

## `alloc`

Allocates uninitialized storage for `count` values of `T`. Initialize it before reading and release the base pointer with `free` when finished.

```dream
public static fun alloc(count: usize): RawPtr<T>
```

## `alloc_zeroed`

Allocates zero-filled storage for `count` values. The caller must release the allocation when finished.

```dream
public static fun alloc_zeroed(count: usize): RawPtr<T>
```

## `alloc_aligned`

Allocates storage for `count` values with the requested power-of-two alignment. Release it with `free`.

```dream
public static fun alloc_aligned(count: usize, alignment: usize): RawPtr<T>
```

## `realloc`

Resizes an independently allocated base pointer, preserving the common prefix. Use the returned pointer and discard all old aliases.

```dream
public fun realloc(new_count: usize): RawPtr<T>
```

## `free`

Releases this allocation. Every other pointer into the allocation becomes invalid.

```dream
public fun free(): void
```

## `copy_to`

Copies `count` initialized values into `dst`. Both ranges must be valid and must not overlap.

```dream
public fun copy_to(dst: RawPtr<T>, count: usize): void
```

## `move_to`

Copies `count` initialized values into `dst`, allowing overlapping ranges. Both ranges must be valid.

```dream
public fun move_to(dst: RawPtr<T>, count: usize): void
```

## `fill_bytes`

Fills `byte_count` writable bytes with `value`. The caller must ensure the range is valid.

```dream
public fun fill_bytes(value: byte, byte_count: usize): void
```
