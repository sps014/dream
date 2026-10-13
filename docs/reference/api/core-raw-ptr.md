# RawPtr

No import is needed.

A non-owning address for unmanaged values. Copies never retain storage. Allocation is uninitialized unless alloc_zeroed is requested. Initialize values before reading. Raw operations require @unsafe and do not check bounds. Only base pointers returned by RawPtr allocation methods may be freed or resized. Foreign allocations require their original deallocator. Reallocation returns a new handle and invalidates every old alias. Null and zero-byte allocations carry address zero; freeing null is a no-op.

[Raw memory guide](../language/arrays.md#rawptrt-raw-memory-unsafe).

## `RawPtr`

```dream
public struct RawPtr<T: unmanaged>
```

## `null`

Constructs address zero without allocating storage.

```dream
public static fun null(): RawPtr<T>
```

## `is_null`

Tests whether the address is zero.

```dream
public fun is_null(): bool
```

## `address`

Returns the target-width integer address.

```dream
public fun address(): usize
```

## `operator`

Equality compares addresses.

```dream
public fun operator ==(other: RawPtr<T>): bool
```

## `from_address`

Wraps an address; the caller supplies its validity, alignment and lifetime.

```dream
public static fun from_address(address: usize): RawPtr<T>
```

## `cast`

Reinterprets the address without changing storage or ownership.

```dream
public fun cast<U: unmanaged>(): RawPtr<U>
```

## `offset`

Offsets count elements; negative offsets are permitted within live storage.

```dream
public fun offset(elements: isize): RawPtr<T>
```

## `byte_offset`

Offsets count bytes. Align the resulting address before typed access.

```dream
public fun byte_offset(bytes: isize): RawPtr<T>
```

## `read`

Reads one initialized value; the returned value is an independent copy.

```dream
public fun read(): T
```

## `write`

Writes one unmanaged value without ARC.

```dream
public fun write(value: T): void
```

## `this`

Unchecked indexing with a signed element offset.

```dream
public fun this[index: isize]: T
```

## `this`

Unchecked indexing with a signed element offset.

```dream
public fun this[index: isize] = value: T
```

## `alloc`

Counts elements. Returned storage is uninitialized.

```dream
public static fun alloc(count: usize): RawPtr<T>
```

## `alloc_zeroed`

Counts elements and zeroes their storage. Zeroed bytes must represent valid T values before reading.

```dream
public static fun alloc_zeroed(count: usize): RawPtr<T>
```

## `alloc_aligned`

Alignment must be a nonzero power of two, at least alignof(T). Counts elements.

```dream
public static fun alloc_aligned(count: usize, alignment: usize): RawPtr<T>
```

## `realloc`

Preserves the overlapping byte prefix and alignment. The grown tail is uninitialized. Assign the returned pointer before using it.

```dream
public fun realloc(new_count: usize): RawPtr<T>
```

## `free`

Releases manually owned storage and clears this handle. Other copies become invalid.

```dream
public fun free(): void
```

## `copy_to`

Counts elements; ranges must not overlap.

```dream
public fun copy_to(dst: RawPtr<T>, count: usize): void
```

## `move_to`

Counts elements; overlapping ranges are supported.

```dream
public fun move_to(dst: RawPtr<T>, count: usize): void
```

## `fill_bytes`

Fills byte_count bytes, independently of the element type.

```dream
public fun fill_bytes(value: byte, byte_count: usize): void
```
