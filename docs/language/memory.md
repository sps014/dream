# Memory Management

Dream allocates heap objects with a **size-class freelist** in WebAssembly linear memory.
A default general-purpose allocator (GPA) is installed at startup. Owning locals are dropped
when they die (`$dream_drop` then `$free`). Implicit `let a = b` for heap types **aliases**
the pointer; `$free` is idempotent so a second drop is safe. **`move`** transfers uniqueness:
the source is nulled and cannot be used afterward. Parameters **borrow** unless declared `move`.

Debug builds count live GPA blocks (`Debug.live_objects`) and trap at process exit if any
remain. Release builds skip that check.

Engineering details: [`docs/compiler/12-allocators.md`](../compiler/12-allocators.md).

## What lives on the heap

- Strings
- Arrays (`T[]`)
- Class instances (including `List`, `Map`, `Set`)
- Capturing closures

Primitives (`int`, `float`, `bool`, ...) and value `struct`s live on the stack or inline
inside other objects — no heap allocation.

`js` handles are host-registry entries. The host unregisters them when the Dream-side handle
is dropped. See [The `js` type](js-type.md).

## How allocation works

`$malloc` either pops a same-class freelist block or bump-extends `HEAP_PTR`. Dropping an
owner runs nested `deinit` then `$free`. Interned string literals have `size == 0` and are
never freed.

```dream
fun make_list(): int[] {
    let arr = [1, 2, 3];
    return arr;
}

fun main() {
    let result = make_list();
    println(result[0]);
}
```

## Choosing an allocator

Most programs never pick an allocator: the GPA is enough. Switch strategy when the *shape*
of allocation is the bottleneck.

| Use | When | What you get |
|-----|------|----------------|
| **Default GPA** | Ordinary objects, long-lived graphs, most stdlib code | Size-class freelist. Drop returns the block for reuse. |
| **Reuse a collection** | Hot loops that fill and empty the same `List` / `Map` / `Set` | `clear()` keeps capacity. Cheaper than `List()` every iteration. |
| **`with ArenaAllocator()`** | A burst of short-lived objects that all die together (parse a file, one request, one frame) | Bump allocate for the block. Individual `$free` of those pointers is a no-op; the slab is released on exit. |
| **`Buffer.alloc` / overwrite** | Fixed-size scratch you replace each iteration | The compiler drops the previous array when the local is reassigned, so the freelist stays hot. |
| **`Pointer<T>` / `Buffer.free`** | You need C-style alloc/realloc/free and accept `@unsafe` | Immediate return to the allocator. You must not use the old pointer afterward. |
| **Value `struct` / scalars / dense `int[]`** | Tight numeric loops | No heap header, better cache behavior. Silent SROA may also promote non-escaping class instances. |

### Default GPA

Use this unless you have a measured reason not to. Create objects normally; owning locals
drop at function exit (and unique arrays drop on overwrite). Do **not** allocate a new
`List`/`string` graph every iteration of a hot loop if you can reuse or arena-scope it.

### Reuse (`clear`, overwrite, `StringBuilder`)

```dream
let xs = List<int>();
let i = 0;
while (i < n) {
    xs.clear();
    xs.push(i);
    // ...
    i = i + 1;
}
```

- `List.clear` / `Map.clear` / `Set.clear` keep the backing buffer.
- Build text with `StringBuilder` (`append`, `append_utf8_slice`) instead of repeated `string` `+`.
- Prefer `byte_size` / `byte_at` when you do not need scalar indices.

### Arenas (`with ArenaAllocator`)

Wrap a scope whose allocations should vanish together:

```dream
fun parse_chunk(src: string): int {
    with ArenaAllocator() {
        let tmp = List<int>();
        // ... fill tmp, compute a scalar result ...
        return tmp.length;
    }
}
```

`with ArenaAllocator(nbytes)` pre-sizes the slab. Do **not** return or store heap pointers
allocated inside the `with` to a caller that outlives the block — those pointers are invalid
after the arena exits. Copy out scalars, or allocate the result on the GPA before/outside
the `with`.

Arenas are the right tool for “allocate a lot, keep almost nothing.” They are the wrong tool
for objects that must survive the scope (caches, long-lived graphs).

### `@unsafe` buffers

`Buffer.realloc` / `Buffer.free` and [`Pointer<T>`](arrays.md#pointert-manual-allocation-unsafe)
talk to `$realloc` / `$free` directly. Every function that touches that lifetime must be
`@unsafe`:

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
single-ownership. Prefer `Span<T>` when you only need a view.

## `move`

```dream
fun take(move p: Point): void { }

fun main() {
    let a = Point();
    let b = move a;  // `a` cannot be used after this
    take(move b);    // transfers into `take`, which drops `p`
}
```

## `del`

A class may define `del()`. Nested drop runs it as **prompt destructor glue** when the
compiler emits `$dream_drop` for that type — not as a delayed finalizer. Use an explicit
`close` / `dispose` for files, sockets, or GPU resources.

## Cycles and `weak`

Aliased heap pointers are not traced. A cycle of owning references is never dropped
automatically. Break cycles with `move`, borrow parameters, an arena that frees the whole
graph, or an explicit teardown. `weak` fields still parse but are not cleared by the
runtime; prefer `borrow` parameters or explicit back-edges.

## WebWorkers

Linear memory is **shared** across the owner and every `WebWorker` (needed for `@shared` /
`lock`). Prefer message-passing for ordinary data; use `@shared` only for intentionally
shared graphs. The allocator takes a lock when threads are present. See [WebWorkers](webworkers.md).

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
