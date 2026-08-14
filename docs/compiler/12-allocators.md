# Design note: allocators

Dream’s heap is an **explicit allocator** in WASM linear memory. User-facing guidance
(when to use the GPA, arenas, reuse, `@unsafe`) lives in
[docs/language/memory.md](../language/memory.md).

## Locked choices

1. **Default allocator** — installed at startup. Debug / `dream test`: tracking GPA (live
   map, canaries, leak report on process/test exit). `--release`: size-class freelist only.
2. **Current allocator** — fiber/worker-local fat pointer; `New` / `T[]` / string concat /
   closures / async frames allocate from it. `with ArenaAllocator() { ... }` switches it.
3. **Owning heap values** drop when the owning local dies (`deinit` then `$free`). Parameters
   borrow by default; `move` on a parameter transfers ownership. No lifetime checker.
4. **Header** — `[size:i32][tag:i32][reserved:i32]` (12 bytes; data at +12). Size `0` marks
   immortal interned strings (`$free` is a no-op).
5. **`weak` is not cleared** by the runtime. `del()` is prompt `deinit`, not a delayed
   finalizer.

## Runtime

`$malloc` / `$free` / `$realloc` in `crates/dream-mir/src/runtime/allocator.wat`. Bump
`HEAP_PTR` (WASM global on the single-thread path) or pop a size-class freelist block.
Blocks in a class are rounded to that class’s size so mixed requests still reuse.
`$dream_drop` walks heap fields / array elements then `$free`. `$arena_enter` /
`$arena_exit` implement `with ArenaAllocator()`. Debug `main` calls `$__gpa_check_leaks`
(`unreachable` if `$live_objects != 0`).

`InsertDrops` (`crates/dream-mir/src/passes/insert_drops.rs`) runs after the per-function
opt pipeline, using whole-module escape info. Unique **array** locals are also dropped on
overwrite so tight `Buffer.alloc` loops reuse the freelist.

## Throughput notes (release)

Arenas match bump throughput for scoped work. Tiny allocate-and-forget on the GPA without
drop or an arena grows the bump pointer and never reuses blocks — reuse, overwrite-drop, or
an arena if that shows up in a profile.
