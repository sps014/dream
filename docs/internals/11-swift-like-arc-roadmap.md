# Design note: Nim-hard ARC

Decision record for ARC performance. **Sink-default ABI**, **cursor locals**, and
**use-after-move** diagnostics are the active model (Nim `--mm:arc` style: compile-time
move/copy/destroy on top of non-atomic RC).

Sibling permanent non-goals remain in force:
[`10-stack-alloc-and-mono-design-note.md`](./10-stack-alloc-and-mono-design-note.md) — no string
SSO, no user-facing `@stack` on class instances, no size-class-keyed unmanaged mono.

## Baseline

- Implicit HIR ownership; explicit MIR `Retain` / `Release` / `New` after `RcInsertion`.
- Call ABI: **unmarked RC params sink; `borrow` shares; caller owns the result (`+1`)**.
  Implicit `this` is never a sink. Call sites **move on last use**, otherwise **retain a copy**
  (Nim sink semantics).
- Cursor locals: non-escaping field/index loads skip retain/release.
- Value `struct` / plain `enum` off-heap (shadow stack); classes / arrays / strings / collections
  on the heap with a 12-byte `[size][tag][ref_count]` header.
- `weak` / `unowned` + structural cycle check; weak teardown via a global registration list
  ([`weak.wat`](../../crates/dream-mir/src/runtime/weak.wat)).
- `RcElision` over Goto chains, transparent diamonds, transparent natural loops, postdom regions
  (never under-retain); last-use move in `RcInsertion` via CFG liveness (including loops).
- `@shared class` atomic retain/release; silent SROA for non-escaping class instances.
- `ref name: T` parameters for mutable place / value-struct aliasing.
- Flow-sensitive use-after-move diagnostics on sink call arguments.

## Locked design choices

1. **Default call ABI is sink-params / own-result.** `borrow` opts out; no user-facing `take`.
   Callers copy into sinks when the argument is still live (no hard use-after-sink error).
2. **Language surface uses `borrow` / `ref`** in the parameter modifier slot.
3. **No OSSA / ownership-SSA rewrite of MIR.** Elision stays statement-level on the CFG.
4. **CoW** (if ever) stays behind an explicit copy API; collections remain classes.
5. **Weak teardown** may stay a global list until profiling shows it hot.

## Non-goals

- User `=copy` / `=sink` operators (`del` remains the destructor)
- Keeping `take` as a synonym of unmarked sink
- String SSO, `@stack` classes, value-struct `List`/`Map`/`Set`
- Atomic RC by default
- CPU SIMD language surface / Dream-owned tiered JIT

## Heap throughput (shipped levers)

ARC alone removes RC *tax*; remaining cost is heap hits and `$malloc`. These are the active
levers (no SSO / `@stack` class / value collections):

1. **Silent SROA** — including post–simple-user-ctor expand (`ExpandSimpleCtors`): non-escaping
   instances with only non-ref field accesses promote to locals (`crates/dream-mir/src/passes/sroa.rs`).
2. **Clear-and-reuse** — `List`/`Map`/`Set.clear` keep capacity and zero live slots in place
   (no capacity-sized realloc). Prefer `clear` + refill over `new` each batch.
3. **ScratchArena** — bump `Span<T>` from one owned slab (`ScratchArena<T : unmanaged>`);
   `reset()` rewinds (`system.core`).
4. **Contiguous / SOA hot paths** — regex Pike-VM uses parallel `List<int>` / `List<int[]>`
   thread queues, reused mark/caps buffers, and `Buffer.elems_copy` for capture clones.
5. **Ownership discipline** — sink-default + `borrow` + field-store use-after-move (see
   [`functions.md`](../language/functions.md)).

Authoring rule: borrow + move + dense memory + clear/reuse → ARC can beat gen0 on the *same*
shapes; `new` + share a class graph every iteration will not.

Measure: `./scripts/run-microbenches.sh` → `tests/bench/out/native.txt` / [`BASELINE.md`](../../tests/bench/BASELINE.md).
