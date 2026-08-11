# Design note: Swift-like ARC performance roadmap

Decision record for ARC performance work. Phases 0–1 are **done**. The remaining
**perf track** is stronger measured elision, then opt-in `take` / `borrow` parameter
modifiers (Dream-native wording; not Swift’s `consuming` / `borrowing`).

Sibling permanent non-goals remain in force:
[`10-stack-alloc-and-mono-design-note.md`](./10-stack-alloc-and-mono-design-note.md) — no string
SSO, no user-facing `@stack` on class instances, no size-class-keyed unmanaged mono.

## Baseline (already shipped)

- Implicit HIR ownership; explicit MIR `Retain` / `Release` / `New` after `RcInsertion`.
- Call ABI: **callee borrows params; caller owns the result (`+1`)**.
- Value `struct` / plain `enum` off-heap (shadow stack); classes / arrays / strings / collections
  on the heap with a 12-byte `[size][tag][ref_count]` header.
- `weak` / `unowned` + structural cycle check; weak teardown via a global registration list
  ([`weak.wat`](../../crates/dream-mir/src/runtime/weak.wat)).
- `RcElision` over Goto chains, transparent diamonds, and transparent natural loops (never
  under-retain); last-use move in `RcInsertion` for dead owned locals outside loops.
- `@shared class` atomic retain/release; silent SROA for non-escaping class instances.
- `ref name: T` parameters for mutable place / value-struct aliasing.

## Impact ranking

| Item | Hot-path effect | Status |
|------|-----------------|--------|
| CFG / loop / move RC elision | Fewer `$retain` / `$release_*` | Done (Phase 1); extend under Track A |
| Retain/release WAT goldens | Measurement + regression guard | Track A |
| `take` params | Sink store without second retain; caller drops +1 | Track B |
| Explicit `borrow` | Documents default ABI | Optional; no extra codegen |
| CoW for collections | Assignment already shares the class ref | Deferred |
| Per-object weak side tables | Weak rare; strong count already inline | Out of perf track |

## Locked design choices

1. **Default call ABI stays borrow-params / own-result.** Unmarked parameters keep today’s meaning.
2. **Language surface uses `take` / `borrow`**, in the same modifier slot as `ref` — not
   `consuming` / `borrowing` / `inout`.
3. **No OSSA / ownership-SSA rewrite of MIR.** Elision stays statement-level on the CFG.
4. **CoW** (if ever) stays behind an explicit copy API; collections remain classes. Not on the
   active perf track.
5. **Weak teardown** may stay a global list until profiling shows it hot.

## How `take` / `borrow` look in APIs

Same position as `ref`:

```dream
public class List<T> {
    // Takes ownership of `value` — caller must not use it afterward.
    public fun push(take value: T): void {
        if (this.count == this.items.length) {
            this.grow();
        }
        this.items[this.count] = value;
        this.count = this.count + 1;
    }
}

fun first_item(borrow list: List<string>): Option<string> {
    if (list.is_empty()) {
        return Option.None;
    }
    return Option.Some(list.get(0));
}

fun demo(list: List<string>) {
    let s = "hi";
    list.push(s);          // moved into the list — caller must not use `s` afterward
    // Use-after-take is caller discipline for now (not yet flow-sensitively diagnosed).

    println(first_item(list)); // borrow — list still usable
    println(list.length);
}
```

| Modifier | Meaning |
|----------|---------|
| *(none)* or `borrow` | Callee borrows; caller keeps ownership (today’s default) |
| `take` | Callee takes the caller’s +1; stores can skip a retain |
| `ref` | Existing mutable place alias — unchanged |

## Phases / tracks

| Phase | Status | Summary |
|-------|--------|---------|
| 0 | Done | This note |
| 1 | Done | Diamonds, transparent loops, last-use move |
| Track A | Done | WAT retain/release goldens + postdom elision / join-safe moves |
| Track B | Done | `take` / `borrow` modifiers + `List.push(take …)` |
| CoW | Deferred | Low hot-path value while collections are classes |
| Weak side tables | Out of perf track | Scalability nicety only |

### Track A — measure + stronger elision

- Goldens with upper bounds on `$retain` / `$release_` in WAT.
- Pair cancel when release post-dominates retain and all paths are transparent.
- Last-use move across simple forward joins when the source is dead (still no loops).
- Never under-retain.

### Track B — `take` / `borrow`

Parse and type-check modifiers; wire `take` through HIR → `RcInsertion` / emitter; annotate
`List.push`. `borrow` is accepted as an explicit spelling of the default. Flow-sensitive
use-after-take diagnostics are deferred (branching and double-push patterns make a
flow-insensitive check too noisy).

## Explicit non-goals

- String SSO / tagged inline strings
- `@stack` on class instances (SROA remains)
- Tracing GC
- Making `List`/`Map`/`Set` into value structs
- Full Swift SIL ownership SSA as a MIR rewrite
- Swift-style `consuming` / `borrowing` / `inout` keywords
- Weak side-table reshape as a performance project

## Success metrics

- Track A: goldens + more cancelled pairs than Phase 1 alone; determinism preserved.
- Track B: `take` sinks show fewer retains at the call+store boundary;
  APIs read naturally next to `ref`. Use-after-take remains caller discipline until
  flow-sensitive checking lands.
