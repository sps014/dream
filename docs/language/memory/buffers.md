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

`T[]` is always safe to use directly — including as the backing storage of your own data structure.
Three guarantees hold for any array, no matter how you track its contents:

1. **Overwriting a slot releases the old element** immediately.
2. **Reading a slot retains the element** — both the array and your variable are valid owners, so there is no way to create a dangling or double-freed reference through reads.
3. **Freeing an array releases every slot**, including slots a counter has already "left behind".

The practical consequence: if you hand-roll a container (`entries: string[]` plus a `count: int`), rewinding the counter **without clearing slots is safe**. Old elements stay until they are overwritten or the array is dropped. There is no error.

Eager clear is optional.

Two opt-ins exist for people writing performance-sensitive containers:

- `Buffer.clear<T>(arr)` / `Buffer.truncate<T>(arr, n)` release elements eagerly (the same cleanup rules as replacing an array item). Use them when one long-lived container churns through many elements and peak memory matters more than microsecond-level cost.
- Zeroing a vacated slot after moving an element out (`items[i] = Buffer.alloc<T>(1)[0]`, like [`List.pop`](../../library/collections/index.md)) reclaims immediately instead of at overwrite/drop.

The genuinely unsafe tier stays behind [`@unsafe`](buffers.md#unsafe-manual-memory-management): `RawPtr<T>` and `Arena` manage unmanaged raw storage; `Buffer.realloc` and `Buffer.free` manually control array lifetime.


## `@unsafe`: manual memory management

A handful of low-level primitives step outside ARC: `Buffer.realloc` / `Buffer.free` and [`RawPtr<T>`](../types/arrays.md#rawptrt-raw-memory-unsafe) manage a block's lifetime yourself.
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
