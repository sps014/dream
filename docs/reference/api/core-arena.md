# Arena

No import is needed.

A manually freed value handle for mixed unmanaged allocations. Initial capacity counts bytes. Growth adds blocks without moving existing allocations. Reset invalidates all pointers and retains every block for reuse; free releases all blocks and state. Arena allocations cannot be individually freed or resized. Copies alias the same state, which must be freed once. Allocation, reset, free and construction require @unsafe.

[Raw memory guide](../language/arrays.md#rawptrt-raw-memory-unsafe).

## `Arena`

Copies alias one manually owned state. Reset/free invalidates every allocation; dropping this value does not free anything, and individual allocations must never be freed.

```dream
public struct Arena
```

## `constructor`

Creates an arena with the requested initial byte capacity. Call `free` when finished.

```dream
public constructor(initial_capacity: usize = 4096)
```

## `alloc`

Allocates storage for `count` values of `T`. Initialize values before reading them.

```dream
public fun alloc<T: unmanaged>(count: usize): RawPtr<T>
```

## `alloc_zeroed`

Allocates zero-filled storage for `count` values. The arena owns it until `reset` or `free`.

```dream
public fun alloc_zeroed<T: unmanaged>(count: usize): RawPtr<T>
```

## `reset`

Discards all arena allocations for reuse. All pointers obtained from this arena become invalid.

```dream
public fun reset(): void
```

## `free`

Releases the arena and all its storage. All copies and allocated pointers become invalid.

```dream
public fun free(): void
```
