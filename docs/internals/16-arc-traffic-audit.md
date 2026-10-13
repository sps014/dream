# ARC traffic audit and fixes

The October 13, 2026 investigation found temporary ownership in collection reads,
removal and comparator calls. These changes address the ownership operations;
they do not establish a new Dream/C# timing ratio.

## Reproduce

```powershell
cargo build --workspace
target/debug/dream --release --crate-type lib --emit-llvm --emit-mir=after:frame-alloc docs/internals/probes/arc_traffic.dream -o target/arc-audit/probe.bin
python scripts/test_corpus.py --both --release arc_borrowing
```

The probe exports independent functions so their MIR and assembly can be inspected.
Explicit retain sites in final MIR changed as follows in this checkout:

| Function | Before | After |
| --- | ---: | ---: |
| Map span lookup | 1 | 0 |
| Ordinary string Map lookup | 0 | 0 |
| Set span lookup | 0, with a value drop across the helper call | 0, no value drops |
| Queue peek followed by string length | 1 | 0 |
| List pop followed by unwrap | 3 | 2 |
| Queue dequeue followed by unwrap | 3 | 2 |
| Call to a non-inlined string length helper | 1 | 0 |
| String List sort helper | 2 | 0 |

Pop/dequeue still return owned values; their remaining transitions include Option
unwrapping. The eliminated retain acquired ownership immediately before clearing
the same slot. `CollectionStorage.take` now transfers the slot's token and zeroes
its storage, including records with managed fields.

## Proof boundaries

`rc-held-by-owner` follows strong fields of by-reference array elements back to
the array owner. It checks both element replacement and writes to the selected
record field, including summarized calls and destructor effects. Weak and
unowned fields are excluded.

For niche Options, payload extraction and local moves join the same borrowing
family. Null-only incoming branches carry no ownership. Returning, storing or
sinking any family member prevents this optimization. Scalar Queue peek can
borrow; an owned snapshot must remain valid after the queue is cleared.

The parameter-mode pass also recognizes closed immutable-string kernels with
scalar results. Exports, async functions, reference parameters, opaque calls,
managed aggregates and global stores remain excluded. The debug ABI is unchanged.

Function-target analysis resolves a callback parameter only when every direct
caller supplies the same proven target. Exported, address-taken, constructor,
async and reassigned parameters cannot use that proof. Analysis runs again after
inlining exposes callback arguments. Known read-only comparators then become
direct calls and the existing lifetime analysis can remove argument snapshots.
Opaque or conflicting callbacks retain their ownership protection. PriorityQueue
field targets still require the existing closed-world constructor invariant;
the exported library probe cannot establish that invariant.

The closed executable probe `probes/arc_comparators.dream` prints `12`. Its final
string PriorityQueue push/pop and List sort helper contain no retain sites or
indirect comparator calls. Run it with `--release --emit-mir=after:frame-alloc`
to inspect the closed-world case separately from the exported library.

## Clear and refill measurements

`map_clear_reuse` measures refill insertion plus clearing, divided by insertion
count. Unmanaged Map clearing already advances an epoch. Changing the table
design on the basis of that combined row would obscure the actual cost.

`tests/bench/map_phases.dream` isolates reserved-capacity refill from batched
clear. Refill excludes clearing and growth; clear excludes refill. The clear row
includes traversal of the batch, so it measures more than a bare epoch increment.

```powershell
target/debug/dream --release run tests/bench/map_phases.dream
```

Use repeated measurements on the same controlled machine before claiming a
speedup or changing insertion checks. The audit's original C# ratios came from
a separate run and are not a baseline for this checkout.
