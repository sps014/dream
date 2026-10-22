# Debug

**Import:** `import system;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Debug`

Allocator introspection for tests and diagnostics.

```dream
public static class Debug
```

## `ref_count`

Returns the current strong reference count of `o` for debugging.

```dream
public static extern fun ref_count(borrow o: object): int
```

## `runtime_counter`

Opt-in attribution (DREAM_RUNTIME_COUNTERS=1); ids follow `dream_ownership.h`. Ordinary builds and unknown ids read as 0. Ids 11, 12 and 13 instead read the native heap's committed, live and free-listed bytes in every build by walking the heap, so each read costs time proportional to the heap size; WebAssembly builds read 0.

```dream
public static extern fun runtime_counter(id: int): long
```

## `free_list_head`

Memory-reuse probe that changes when the allocator frees storage. Release builds do not track frees and always read 0.

```dream
public static get free_list_head(): int
```

## `heap_ptr`

Current heap address exposed for debugging.

```dream
public static get heap_ptr(): int
```

## `live_objects`

Number of currently live managed allocations. Only debug builds count allocations; release builds always read 0.

```dream
public static get live_objects(): long
```

## `total_allocations`

Total number of managed allocations made so far. Only debug builds count allocations; release builds always read 0.

```dream
public static get total_allocations(): long
```
