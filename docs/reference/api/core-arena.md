# Arena

No import is needed.

A manually freed value handle for mixed unmanaged allocations. Initial capacity counts bytes. Growth adds blocks without moving existing allocations. Reset invalidates all pointers and retains every block for reuse; free releases all blocks and state. Arena allocations cannot be individually freed or resized. Copies alias the same state, which must be freed once. Allocation, reset, free and construction require @unsafe.

[Raw memory guide](../language/arrays.md#rawptrt-raw-memory-unsafe).

## `Arena`

```dream
public struct Arena
```

## `constructor`

```dream
public constructor(initial_capacity: usize = 4096)
```

## `alloc`

Counts elements. Returned storage is uninitialized.

```dream
public fun alloc<T: unmanaged>(count: usize): RawPtr<T>
```

## `alloc_zeroed`

Counts elements and zeroes their storage. Zeroed bytes must represent valid T values before reading.

```dream
public fun alloc_zeroed<T: unmanaged>(count: usize): RawPtr<T>
```

## `reset`

Invalidates all previous allocations and rewinds every block for reuse.

```dream
public fun reset(): void
```

## `free`

Releases manually owned storage and clears this handle. Other copies become invalid.

```dream
public fun free(): void
```
