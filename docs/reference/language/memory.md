# Memory Management

Dream manages heap memory with a **stop-the-world generational garbage collector** in
WebAssembly linear memory (Gen0 nursery, Gen1, Gen2, and a large-object heap). You never
call `free` for ordinary values — unreachable objects are reclaimed when the collector runs.

Engineering details: [`docs/internals/12-tiered-gc.md`](../../internals/12-tiered-gc.md).

## What lives on the heap

- Strings
- Arrays (`T[]`)
- Class instances
- Standard library collections (`List`, `Map`, `Set`)

Primitives (`int`, `float`, `bool`, ...) and value `struct`s are stored on the stack or
inline inside other objects — no heap allocation.

`js` handles are host-registry entries. The Dream-side handle is a GC-managed value; when
it becomes unreachable, the host unregisters the entry. See [The `js` type](js-type.md).

## How it works

The runtime allocates short-lived objects into a **Gen0 nursery**. When the nursery fills,
a short stop-the-world collection copies survivors into Gen1 (and may promote further into
Gen2). Large allocations go to the **LOH** and are swept rather than copied. Cycles are
collected normally — there is no reference-count balance to break.

```dream
fun make_list(): int[] {
    let arr = [1, 2, 3];   // nursery allocation
    return arr;
}

fun main() {
    let result = make_list();
    println(result[0]);
} // when `result` is no longer reachable, GC reclaims it
```

Pauses are typically dominated by Gen0; full Gen2 / LOH collections are rarer and longer.
Prefer clear-and-reuse (`List.clear`) and `ScratchArena` on hot paths to allocate less.

## Finalizers: `del`

A class may define `del()`. Under GC it is a **finalizer**: after the object is found
unreachable, the runtime may run `del` before reclaiming the block. Timing is not
guaranteed — do not use `del` for prompt release of files, sockets, or GPU resources;
prefer an explicit `close` / `dispose` when those APIs exist.

If a finalizer stores `this` into a still-reachable location (resurrection), the object
stays alive and `del` is not run again for that object.

## `weak` references

Mark a field `weak` when it should not keep its target alive (caches, parent back-edges,
observer lists):

```dream
class Node {
    public value: int;
    public weak parent: Option<Node>;
}
```

- **`weak T`** — must be `Option<T>` for a class `T`. After the referent becomes
  unreachable, the slot is cleared to `Option.None` before finalizers run.
- There is no `unowned` modifier.

Cycles through ordinary strong fields are fine; the collector traces them.

## `@unsafe`: manual memory management

A handful of low-level primitives step outside the GC for tight, allocation-sensitive hot
paths: `Buffer.realloc`/`Buffer.free` and [`Pointer<T>`](arrays.md#pointert-manual-allocation-unsafe)
manage a block's lifetime directly through the allocator. Every function or method that
touches one of these must be marked `@unsafe`:

```dream
@unsafe
fun grow(p: Pointer<int>): Pointer<int> {
    p.realloc(p.length * 2);
    return p;
}

fun caller(): void {
    let p = Pointer<int>.alloc(4);
    grow(p);   // error: call to '@unsafe' function 'grow' is only allowed from
               // another '@unsafe' function or method
}
```

`@unsafe` is a caller-side gate only — it does not insert runtime checks or prove
single-ownership. It is a documented promise, not a proof.

## Performance notes

- Prefer `StringBuilder` (and `append` / `append_utf8_slice`) over repeated `string` `+`
  when building text in a loop.
- Use `byte_size` / `byte_at` / byte-oriented helpers when you don't need scalar indices.
- `List` / `Map` / `Set` `clear()` keeps capacity: live slots are zeroed in place. Prefer
  `clear` + refill over allocating a new collection each batch.
- `ScratchArena<T : unmanaged>` bump-allocates short-lived `Span<T>` slices from one owned
  slab; `reset()` rewinds the cursor without freeing to the OS.
- `Span.copy_from` on `unmanaged` element types bulk-blits with `memory.copy`; managed
  elements go through ordinary assignment (write barriers apply).
- Prefer `struct` / scalars / `Span` / dense `int[]` on hot paths; silent SROA may promote
  non-escaping class instances whose accessed fields are non-references.
- Under `--release`, the inliner erases small `Span` / value-struct method call boundaries.

## WebWorkers

Linear memory is **shared** across the owner and every `WebWorker` (needed for `@shared` /
`lock`). The GC is stop-the-world and cooperative: collections run only at safepoints while
the allocator lock is held. Prefer message-passing for ordinary data; use `@shared` only for
intentionally shared graphs. See [WebWorkers](webworkers.md).

## WASM call stack (`dream run`)

This is the **WebAssembly guest call stack** (wasmtime `max_wasm_stack`), not
language-level stack allocation of Dream values. Deep recursion and large value-struct
frames need enough of it.

Precedence when running with the native host (`dream run` / e2e):

1. Environment variable **`DREAM_STACK_SIZE`** — e.g. `32M`, `32MiB`, `33554432` (bytes).
2. Compiler default baked from the Dream repo’s `[package.metadata.dream] stack-size`
   (currently `16MiB` in the root `Cargo.toml`).
3. Hard fallback: **16 MiB**.

```bash
DREAM_STACK_SIZE=32M dream run path/to/file.dream
```

Async fibers get a few MiB above that sync budget. Values below 64 KiB are rejected.

This only affects the **native** wasmtime runner. Browser / Node hosts use the engine’s
own stack limits. Separately, building or testing the compiler itself may need a larger
**Rust host** thread stack (`RUST_MIN_STACK`, also set in the repo’s `.cargo/config.toml`)
— that is unrelated to guest WASM stack size.
