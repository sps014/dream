# Debug

**Import:** `import system;`

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Debug`

Allocator introspection for tests and diagnostics.

```dream
public static class Debug
```

## `host_free_list_head`

Allocator free-event probe used by `Debug.free_list_head`.

```dream
public static extern fun host_free_list_head(): int
```

## `host_heap_ptr`

Allocator position probe used by `Debug.heap_ptr`.

```dream
public static extern fun host_heap_ptr(): int
```

## `host_live_objects`

Returns the diagnostic live-object count exposed by `Debug.live_objects`.

```dream
public static extern fun host_live_objects(): long
```

## `host_total_allocations`

Returns the allocation count exposed by `Debug.total_allocations`.

```dream
public static extern fun host_total_allocations(): long
```

## `ref_count`

Returns the diagnostic strong-reference count of an object.

```dream
public static extern fun ref_count(borrow o: object): int
```

## `runtime_counter`

Opt-in attribution (DREAM_RUNTIME_COUNTERS=1); ids follow `dream_ownership.h`. Ordinary builds and unknown ids read as 0.

```dream
public static extern fun runtime_counter(id: int): long
```

## `free_list_head`

```dream
public static get free_list_head(): int
```

## `heap_ptr`

```dream
public static get heap_ptr(): int
```

## `live_objects`

```dream
public static get live_objects(): long
```

## `total_allocations`

```dream
public static get total_allocations(): long
```
