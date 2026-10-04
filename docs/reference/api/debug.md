# Debug

**Import:** `import system;`

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Debug`

Allocator introspection for tests and diagnostics.

```dream
public static class Debug
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
