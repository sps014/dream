---
title: Raw buffers
summary: Manual memory operations for custom containers.
icon: material/database-outline
status: stable
targets: [native, web, node]
---

# Work with raw buffers

Use ordinary arrays first. This guide explains the extra care required when building a custom container or using manual memory operations.

!!! glance "At a glance"

    ```dream
    @unsafe
    fun grow(p: RawPtr<int>, count: usize): RawPtr<int> {
        return p.realloc(count);
    }
    ```

    - Prefer arrays and lists; raw buffers are for custom containers.
    - Manual operations are only callable from other `@unsafe` code.

[Back to overview](index.md)

## Raw buffers and custom containers

`T[]` is safe to use as the storage of your own container.

- Writing a slot drops the old element.
- Reading a slot gives you a valid value. The array still holds its own.
- Dropping the array drops every slot, including ones your counter no longer counts.

Rewinding `count` without clearing slots is safe. Old elements stay until they are overwritten or the array is dropped.

Use `List<T>` when the logical size changes. Removing an element or calling `clear()`
releases the list's ownership of its elements. When an array needs empty slots, choose
an optional element type and assign `Option.None` to release a slot's previous value.

## `@unsafe`: manual memory management

[`RawPtr<T>`](../types/arrays.md#rawptrt-raw-memory-unsafe) and `Arena` provide raw storage for unmanaged values. You manage the lifetime of this storage yourself.
Every function or method that touches one of these must be marked `@unsafe`:

```dream
@unsafe
fun grow(p: RawPtr<int>, new_count: usize): RawPtr<int> {
    return p.realloc(new_count);
}

fun caller(): void {
    let p = RawPtr<int>.alloc(4);
    grow(p, 8); // error: call to '@unsafe' function 'grow' is only allowed from
// another '@unsafe' function or method
}
```

Calling an `@unsafe` function from ordinary code is a compile-time error.
Marking your own function `@unsafe` means *its* callers must be `@unsafe` too — the attribute has to be threaded all the way up to wherever the unsafe operation is justified.

`@unsafe` does **not** insert runtime checks, and it does not verify the contract of the operation you're calling (e.g. that a freed `RawPtr<T>` is never read again).
It is a documented promise from the author, not a proof.
