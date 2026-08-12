# Design note: C#-like tiered GC

Decision record for Dream’s heap. **Automatic reference counting is deleted.** Heap
memory is managed by a custom stop-the-world generational garbage collector in WASM
linear memory (not WasmGC). Pointers remain ordinary `i32` data pointers into that
memory.

Sibling permanent non-goals remain in force:
[`10-stack-alloc-and-mono-design-note.md`](./10-stack-alloc-and-mono-design-note.md) — no string
SSO, no user-facing `@stack` on class instances, no size-class-keyed unmanaged mono.

## Why GC

ARC made ownership a whole ABI: MIR `Retain`/`Release`, sink/borrow call rules, cycle
diagnostics, deep `$release_*`, and host-side `js` handle counts. Cycles, async frames,
closures, and UI graphs all paid that tax. A tracing GC removes the balance discipline;
roots and write barriers become the correctness surface instead.

## Locked design choices

1. **Custom GC in linear memory** — keep `i32` data pointers; no WasmGC proposal types.
2. **C# workstation shape** — Gen0 nursery → Gen1 → Gen2 + **LOH**; ephemeral collections first.
3. **Stop-the-world**, precise mark (copying nursery; mark-compact older gens; mark-sweep LOH).
4. **Big bang** — no ARC shims, dual paths, or “legacy” retain/release fallbacks.
5. **`del()` is a finalizer** — run after an object is found unreachable (not guaranteed prompt).
6. **`weak` stays**; **`unowned` is deleted** (dangling under GC is unsafe with no upside).
7. **Value structs, SROA, ScratchArena, clear-and-reuse** stay — they still win under GC.
8. **Workers v1: cooperative STW on shared memory** — linear memory stays `shared` (required for
   `@shared` / `lock` / atomics). Collection takes the allocator lock and requires every instance
   to reach a safepoint before evacuating. Cross-worker live `@shared` object graphs remain
   supported; threads must not mutate the heap during STW.

## Object header

Every managed heap block:

```
[size:i32][tag:i32][gc_meta:i32]  — HEAP_HEADER_SIZE = 12
data pointer = block + 12
```

| Word | Role |
|------|------|
| `size` | Total block bytes (header included), same as today |
| `tag` | Type tag (`TAG_*` / struct tags) for mark visitors and `$object_tag` |
| `gc_meta` | Generation, mark, forwarded, LOH, finalize bits (see `abi.rs`) |

Interned / immortal strings keep `size == 0` and are never moved or swept.

Unmanaged `@unsafe` `Buffer` / `Pointer` blocks bypass the GC (manual `$malloc`/`$free`).

## Generations

| Space | Alloc | Collect | Promote |
|-------|--------|---------|---------|
| **Gen0** | Thread-local bump nursery | Copying evacuate survivors | → Gen1 |
| **Gen1** | Survivors only | Mark + compact | → Gen2 |
| **Gen2** | Long-lived survivors | Full mark + compact/sweep | stays |
| **LOH** | Payload ≥ `LOH_THRESHOLD` (~85 KiB) | Mark-sweep (no copy in v1) | stays |

Triggers:

- Gen0 full → ephemeral collection (Gen0, optionally Gen1).
- Allocation budget / Gen1 pressure → include Gen2.
- LOH pressure → include LOH in the collection.

Prefer short Gen0 pauses (gamedev + web). Incremental / concurrent GC is **post-merge**.

## Roots (precise)

No Boehm-style conservative scan — WASM makes that unreliable.

Compiler + runtime maintain:

1. **GC root shadow slots** — live heap-ref locals, params, and return temps registered at
   safepoints (extends the existing value-struct shadow-stack region conceptually; root
   slots live in a dedicated table scanned by the collector).
2. **WASM globals** that hold refs.
3. **Async Future frames** and **funcbox envs**.
4. **Static / interned strings** — immortal / non-movable.

Value-`struct`s with embedded refs: their slots are already on the shadow stack and are
included in root maps.

**Safepoints** at calls, loop backs, and alloc slow paths poll “collection requested” and
ensure root maps are up to date before STW.

## Write barriers

Every heap store of a reference (field / index / box):

- If storing a **younger** pointer into an **older** object, record a remembered-set
  entry so ephemeral collections can find Gen0 refs from older gens.
- Emitter emits `$write_barrier` instead of retain/release-old.
- Remset capacity is fixed (`GC_REMEMBERED_CAP`). On overflow the barrier **keeps**
  existing entries, sets `GC_REMSET_OVERFLOW`, and requests a Gen0 collect. That
  collection scans **all live old/LOH objects** for young pointers (not only the
  remset). Never reset the remset count on overflow — doing so dropped edges and
  corrupted Map/JSON under a 256 KiB nursery.

## Nursery sizing

Default [`NURSERY_SIZE`](../../crates/dream-mir/src/abi.rs) is **1 MiB**. Remset overflow
must not drop edges (see above); blittable arrays use `TAG_FLAT_ARRAY` so Gen0 does not
treat `int[]` payloads as pointers; heap field stores compute the place address **after**
`$malloc` so evacuated bases are reloaded first.

## Allocation path

- Fast path: Gen0 bump into the nursery (or `$__gc_alloc_old` with LOH gen bits when
  `payload ≥ LOH_THRESHOLD`).
- Slow path: nursery full or `GC_REQUEST_ADDR` set → `$__gc_collect_locked` (kind 0,
  ephemeral) → retry bump. Still no room after the retry → fall back to `$__gc_alloc_old`
  with Gen1 gen bits so the caller cannot spin forever waiting for space that only a
  heavier collection would free.
- Dead objects reclaimed by the collector, not by `$release` → `$free`.
- Freelists may hold **post-GC free space** for Gen1/2/LOH; they are not RC teardown.

### Mutator safepoint reload after calls and allocations

Every reference held in a WASM local, WASM global, `$__obj` scratch, or on the operand
stack is a raw `i32` pointer. During a Gen0 collection the collector updates the shadow
**root table** in place, but any stale copy in the mutator's own registers must be
refreshed before use. The emitter therefore reloads roots after every safepoint:

- After `$malloc`, `$realloc`, `$concat_strings` (allocation).
- After every direct, interface, indirect, and JS call (nested Dream mutator may allocate).
- After a `Rvalue::New` constructor call — the object under construction is rooted through
  `$__obj_rg` for the duration of the call and reloaded from the root table afterward.
- Reload always ends in `$__gc_reload_globals`, which forwards each reference-typed module
  global from its `$__grootN` root slot back into `$gN` before the mutator resumes.

Reloads are gated on [`GC_EPOCH_ADDR`](../../crates/dream-mir/src/abi.rs): Gen0 and old
collections bump the epoch; each function caches the last-seen value in `$__gc_epoch` and
skips the reload body when unchanged (load + compare on the no-collect fast path).

MIR call arguments are already materialized operands (`local.get` / const), so they cannot
allocate while sitting on the WASM operand stack; the callee prologue roots params after
the `call` transfers them into locals. Functions with no call/alloc safepoint skip the
root-table prologue entirely.

## Finalizers / `del`

After mark finds an object unreachable:

1. If the finalize bit is set (type has `del`), enqueue it.
2. Clear weak slots pointing at it.
3. Run finalizers **after** STW mark (objects still allocated while `del` runs).
4. If a finalizer stores `this` into a rooted location (**resurrection**), clear the
   finalize bit once and keep the object; do not re-run `del` for that object.
5. Otherwise free / sweep the block after `del` returns.

Do not rely on `del` for prompt resource release (files, GPU); prefer explicit
`close`/`dispose` APIs when those land.

## `weak` / cycles / `unowned`

- Structural cycle checker and `@allow_cycle` are **deleted** — tracing GC collects cycles.
- **`weak T`**: GC weak ref; cleared after mark, before finalizers (slot becomes `Option.None`).
- **`unowned`**: **deleted** from the language (no fallback).

## JS handles

Dream `js` values are host-registry entries. Lifetime follows GC reachability of the
Dream-side handle:

- No MIR `js_retain` / `js_release` paired with local scopes.
- Host unregisters the entry when the Dream wrapper / handle object is finalized
  (or when the small handle box is collected).

## Call ABI (post-ARC)

- Heap references are plain shared refs; unmarked params are not sinks.
- **`borrow`** is deleted as an ownership modifier (parse/sema reject or treat as plain).
- **`ref`** remains for mutable place / value-struct aliasing.
- No use-after-move-on-sink diagnostics.

## MIR / pass pipeline

1. Lower HIR → MIR (`New` / assign / call — no `Retain`/`Release`).
2. **RootSlots** / safepoint insertion.
3. Inline + existing opts (barriers are side effects; no RC special cases).
4. Emit: alloc → nursery/LOH; stores → write barrier; no scope-exit release.

## Workers policy (v1)

**Shared memory + cooperative STW:** owner and every `WebWorker` import the same linear memory
(required for `@shared` / `lock`). A collection acquires the allocator lock, sets a global
“GC requested” flag, and waits until all live instances have acknowledged a safepoint (or,
in the single-threaded case, runs immediately). Gen0 evacuation and older-gen compaction only
run while no mutator is between safepoints. Message-passing (`string` copies) remains the
safe default for non-`@shared` data.

## What was deleted

| Removed | Replacement |
|---------|-------------|
| MIR `Retain` / `Release` | Alloc + barriers + roots |
| `RcInsertion` / `RcElision` | Root/safepoint pass |
| Sink / `borrow` ownership ABI | Plain shared refs; keep `ref` |
| `$retain*` / `$release_*` | `$gc_alloc`, `$gc_collect_*`, `$write_barrier`, mark visitors |
| Cycle diagnostics / `@allow_cycle` | Gone |
| `unowned` | Gone |
| `Debug.ref_count` | GC stats / forced-collect test APIs |

## Measure

`./scripts/run-microbenches.sh` → `tests/bench/out/native.txt` / [`BASELINE.md`](../../tests/bench/BASELINE.md).
Update BASELINE when GC replaces ARC numbers.
