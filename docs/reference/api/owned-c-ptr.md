# OwnedCPtr

**Import:** `import system;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `OwnedCPtr`

A C pointer returned by an `@owned("free_fn")` `@c` extern: Dream owns it, and the destructor calls `free_fn(ptr)` unless `take()` handed ownership back first.

```dream
public class OwnedCPtr
```

## `free_with`

Releases `ptr` by calling the foreign function at address `free`.

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
