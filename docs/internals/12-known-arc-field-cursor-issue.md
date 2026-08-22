# Known ARC issue: field-read cursor vs. container-slot overwrite

## Status: open (crash), fix requires RC-pipeline rework — not a one-line patch

## The bug

```dream
class Job { public name: string; public constructor(n: string) { this.name = n; } }
class Q2 { public head: Option<Job>; public constructor() { this.head = Option.None; } }

fun main(): void {
    let q = Q2();
    q.head = Option.Some(Job("j"));
    let taken = q.head;        // read slot into a local
    q.head = Option.None;      // overwrite the same slot
    System.println(taken.unwrap().name);   // use-after-free
}
```

Compiles and runs, then segfaults. Repro file kept at `/tmp/t11.dream` shape; verified present at
commit `635f456c` via a clean HEAD worktree build.

## Root cause

`RcInsertion`'s cursor inference (`crates/dream-mir/src/passes/rc/cursor.rs`) marks a
single-assignment local whose value comes from a field/index load (`is_cursor_source`) as a
**cursor**: a non-owning alias, skipped by retain/release insertion. That is sound only while
the source *slot* keeps holding the object. A later store to the same slot (`q.head = …`)
emits `rc_store`, which releases the slot's previous occupant — dropping the last reference
while the cursor still points at the freed block.

## Fix attempted and why it did not land

Converting such cursors to **owners** (escape from cursor candidacy when their source slot is
stored to anywhere in the function; flow-insensitive) fixes the repro (`Retain` after the read,
release balanced by the slot store). It also crashes `tests/cases/http_get_local.dream`
(JSONPlaceholder e2e) with heap corruption in `dream_malloc`. Isolation results:

- Not `RcElision` cancelling the new pairs (`DREAM_RC_NO_ELISION` still crashes).
- Not async-specific (escapes scoped to sync functions still crash — JsonValue/Map helper
  methods are sync).
- Not index-read-specific (field-only escape still crashes).
- **Any** ownership conversion of field/index reads in that program crashes, i.e. owned
  locals sourced from container reads are mishandled somewhere in the token-lattice /
  unique-destroy / insertion pipeline independent of this issue. The cursor optimization has
  been masking that deeper bug.

## Proper fix (future work)

1. Root-cause why an owned local defined by `Assign(v, Use(Copy(Place::Field{…})))` +
   inserted `Retain(v)` corrupts the heap in Map/JsonValue-shaped code (suspects:
   `apply_stmt_unique`/unique-destroy treating the read as a container move; async
   resume-block spills of newly-owned locals; backend `rc_store` interplay for
   borrowed-vs-move stores).
2. Then apply the slot-overwrite escape from cursor candidacy (implementation sketch was
   validated to fix the repro: record `(base, field)` per candidate, escape readers of any
   stored-to slot; flow-insensitive is sound-by-construction for "never under-retain").
3. Add regression goldens: the repro above (positive), plus `http_get_local` parity.

Constraint noted: `backend/c` was being actively modified in parallel while this was
investigated; revisit on a quiet tree.
