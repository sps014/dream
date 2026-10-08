# Compilation profiles and deterministic ownership

Dream exposes exactly two compilation profiles: Debug and Release. Profile selection is
independent of debug information (`-g`), explicit backend optimization (`-O`), and artifact
requests. `CompileProfile` in `dream-abi` is the shared policy identity.

Debug performs mandatory constructor and closure ABI normalization, ownership argument
normalization, retain/release insertion, and ownership verification. It runs one bounded
cleanup round, without whole-program inlining, region inference, managed SROA, frame
allocation, loop optimization, or expensive ARC elision. Debug with `-g` preserves source
locals and call frames. LLVM defaults to O0; program IR is compiled separately and linked
with cached runtime unit objects. Optimized IR and assembly are emitted on request.

Release retains aggressive MIR, ARC, and LLVM optimization. Explicit `-O0` tunes LLVM
without changing the Release ownership preparation or selecting another profile. Both
profiles have identical ownership semantics, including destructor ordering, weak invalidation,
and synchronous cycle reclamation. Cleanup boundaries are established before optimization.
Immortal singletons preserve their reference-count sentinel, leave cycle tracking, and remain
accessible through weak handles bound before pinning. Repeated pinning changes accounting once.

## Cache correctness

An unchanged-build lookup precedes parsing. Stored input fingerprints cover source contents,
manifest and lockfiles, resolution candidates and directories, options, tool identities, and
artifact contents. Corrupt or incomplete entries miss. Builds with warnings are not cached.
Native sources remain excluded until their header dependencies can be recorded. Incremental
generators record their additional inputs and materialized outputs, allowing an unchanged build
to skip parsing and generator execution. Generators with untracked external inputs and aliased
module discovery builds remain excluded until all external inputs can be validated.

Runtime C units record clang's transitive dependencies, including system headers. Unit entries
verify input contents and output contents under cross-process locking, then publish atomically.
Linkers consume immutable object snapshots with native `.o` or LLVM `.bc` suffixes. Verified
header contents are reused within a process only while filesystem identity and change timestamps
remain unchanged; include inventories also detect directory replacement. Debug compiles independent
runtime units in a pool capped at four workers, then preserves catalog order when linking.
Unknown include inventories and external compiler overrides force misses. WASM untyped
allocations use the shared allocator because their destruction returns storage to its free lists;
small blocks fill an existing size class to avoid quadratic free-list churn during reclamation.
WASI C allocator bookkeeping is excluded from Dream object diagnostic counters, matching native
libc allocations; the underlying storage remains reusable and is freed through the same allocator.
Runtime signatures remain derived from the runtime bitcode and are cached independently of
program linking. Cache keys include target, compiler identity, effective flags and instrumentation.

## Cycles

Concrete ownership layouts classify cycle capability; recursive reference types are valid.
Exact visitors enumerate only strong edges, including active union fields and references in
inline values and arrays. Publication uses these visitors rather than payload scanning.
Localized trial deletion subtracts internal edges from scratch counts and retains the graph
reachable from remaining external owners. Traversal is iterative. Ordinary acyclic retain and
release operations on types without user finalizers do not allocate collector state or acquire
its gate. Classes with user finalizers also use managed metadata to distinguish temporary
finalizer borrows from forbidden resurrection, even when their fields cannot form a cycle.
Zero-count cycle-capable
objects use direct iterative teardown without trial counts or ordering scratch: no incoming
strong edge can remain, so the object cannot belong to a cycle.

Cycle reference operations and strong-edge mutations use a runtime gate. Doomed objects are
claimed under the gate; user destructors run after releasing it. Weak and unowned handles are
invalidated before finalization. Finalizers run once in allocation order while peer storage
and edges remain readable. Edges are cleared and storage reclaimed afterward. Temporary
finalizer reads cannot revive ownership; publication and mutation of doomed objects are rejected.
Reentrant destruction joins the current drain. Explicit `defer` postpones collection, and its
final drain must precede leak reporting.

Async frame visitors see ownership tokens rather than transient pointer copies. Pure copy,
retain, move, and source-clearing steps publish atomically under the gate. Consumed call
arguments leave the frame before application code runs; their active stack tokens remain
external owners until transferred. The gate never spans an arbitrary call or cast evaluation.
Lazy async closures retain their environment in a visited frame slot at creation. Polls read
that snapshot, then release it at their first suspension or completion after acquiring the
captured cells. Dropping an unpolled future releases the snapshot as well. Completed async
worker replies acquire their own wire-string token before the result future is released.

Inferred `UniqueRegion` remains a Release optimization for proven nonescaping, destructor-free
graphs. Its private-allocation proof allows recursive types without registering their private
instances in the collector; escaped instances retain ordinary cycle management. Future
explicit graph ownership can define a lifetime for deliberately shared cyclic graphs; no new
syntax or API is introduced here. Determinism concerns the same synchronized execution;
concurrent scheduling has no added global ordering guarantee.

## Release recovery mechanisms

Fresh, statically described objects remain isolated until their first strong edge creates or
shares a pooled component descriptor. Dynamic objects start with a suspect descriptor.
Strong edges join components under the
ownership gate; an edge within a component marks it potentially cyclic. Membership stays
conservative after removal. Weak and unowned edges do not join components. Unknown-owner
mutations invalidate existing components through an epoch and increment an opt-in counter.
An allocation-sequence watermark also invalidates older isolated objects; newly allocated
objects and reused metadata slots do not inherit that fallback.
Parent metadata links own references, so representatives outlive their original objects.
Queued collector nodes remain pinned until their candidates drain, preventing pooled reuse.

Release batches verified field-only initializers after evaluating their arguments. Tracked
initializers join strong components while holding one gate; private initializers omit those
checks only inside an active proved region. Callback-bearing constructors keep the ordinary
path. Fresh recursive builders with scalar inputs and a fully verified call graph can hold
one outer gate. Fresh weak forests also qualify when their constructors only zero fields and
no weak observation, callback, or publication can observe delayed cleanup during construction.
Redundant null writes into the zeroed allocation disappear; constructors that execute weak
operations keep the ordinary path. Nested batches borrow the outer gate rather than
reacquiring it; verified builders also reuse that token for field-only constructor calls. Proven private
recursive builders select an internal clone after the outer region check, carrying the same
proof through recursive calls without checking the region or gate at every node.
The tracked builder clone similarly borrows the outer gate through recursive calls. Its
ordinary entry can acquire a temporary component metadata owner shared by the fresh graph,
then relinquishes it and drains the gate before returning to unproved callers. Verified
initialization of a fresh owner cannot close a cycle, including when its children share
a component; ordinary mutation still marks an existing component potentially cyclic.
Builders with ordinary field mutations retain distinct-component bookkeeping instead of
sharing the temporary descriptor, so weak forest stores retain their inexpensive path.
No gate spans arbitrary application code. Debug keeps ordinary validation. Release emits
collector-free retain and decrement calls for exact acyclic static layouts. Missing layouts,
erased objects, interfaces, and closures stay conservative. Shared counts still use atomic
operations, and canonicalized release wrappers retain their runtime collector check. Weak
registrations and target claims use the same collector gate; their table does not need a
second mutex. Nested weak operations borrow the gate without ending the outer cleanup boundary.

Release span borrowing also admits fresh private string owners. Ownership dataflow proves
that the original owner remains alive at every view and derived-reference read, including
joins and back edges. Escape analysis includes the source fields of inline views, so an
escaping view or published source prevents this specialization. Ownership observations and
ordinary opaque calls retain the checked path. A bounds-check panic cannot access the private
source through its hook; the normal owner remains alive until abort. Debug retains the
ordinary validation path, and the optimization never postpones the original owner's cleanup.

Type metadata separates finalization, clearing, reclamation, and a proof that clearing invokes
no user code. Destructor-free zero-count objects with that proof can clear and reclaim under
the gate; reference children append to the iterative drain. Inline values with destructors
are excluded, and all user finalizers run outside the gate. Deferred erased releases retain
live weak observations until their pending ownership decrement actually reaches zero.

WASM libc storage and worker stacks use an uncounted runtime heap path. Guest object
diagnostics read a single guest counter rather than subtracting independently updated raw
allocation counters; asynchronous worker-stack teardown cannot change guest leak counts.

Runtime event counters are compiled only with `DREAM_RUNTIME_COUNTERS=1`, which changes the
runtime cache identity. Uninstrumented paired timing runs are separate from counter runs.
The existing comparison harness stores versioned full samples and immutable reference
artifacts; missing, corrupt, incompatible, zero-time, or inconsistent-sink results fail
validation. A paired 95% interval above 1.10 fails; an overlapping interval is inconclusive.
The hosted performance workflow publishes measurements, while a configured controlled
runner is required for the hard gate. These mechanisms alone do not establish acceptance.

## Validation

Measure cold runtime builds, warm edited builds, unchanged builds, compiler rebuilds, and
Release builds separately. Keep repeated raw timings, phase costs, memory and subprocess
counts. Architectural changes alone do not establish a speedup. Test native/WASM cleanup
parity and byte-identical repeated artifacts; preserve Release runtime benchmarks. Required
gates are workspace build, strict Clippy, workspace tests, the full native corpus, parity,
runtime layering checks, relevant ignored DAP/toolchain tests, and Linux sanitizer CI.

### Historical measured results and remaining performance cost

The [recorded samples](build-profile-measurements.json) compare release-built host compilers
against commit `2eb32e25` on macOS arm64, with three quiet repetitions and a generated
200-function program. Debug median cold compilation fell from 6.894 s to 3.715 s,
edited compilation from 1.078 s to 0.741 s, and unchanged compilation from 0.231 s to
0.114 s. Debug with `-g` measured 3.451 s cold, 0.690 s edited, and 0.112 s unchanged.
These samples precede the final zero-count teardown and WASI counter changes; they
measure isolated Dream caches, without flushing operating-system filesystem caches.

Release measured 7.000 s cold versus 6.259 s before, 0.810 s edited versus 0.980 s,
and 0.096 s unchanged versus 0.193 s. Cold timings vary substantially; no cold Release
speedup is established. Debug cold peak resident memory rose from a median 66.3 MiB
to 89.5 MiB with parallel runtime compilation. Edited memory rose from 58.5 MiB to
68.6 MiB; unchanged memory fell from 26.7 MiB to 16.8 MiB. The samples contain phase
costs where instrumentation exists and subprocess counts for every repetition.

Debug traces contain no `llvm-link` and no LLVM optimization passes. Cold builds invoke
38 runtime clang compilations, one signature disassembly and one program `llc`; edited
builds invoke no runtime clang compilation. The remaining `opt` subprocess queries its
version. Unchanged builds skip compiler phases and generators. Release retains bitcode
linking and optimization. Three warm incremental rebuilds of the release host compiler after
touching `src/main.rs` took 5.55 s, 3.79 s and 3.39 s, with median peak resident memory
257.6 MiB and one Cargo-reported rustc invocation each. There is no baseline Rust compiler
rebuild comparison, and internal linker subprocesses are not counted by this measurement.

Release runtime performance is not fully preserved. Repeated microbenchmarks showed
numeric kernels close to baseline, but recursive tree allocation/reclamation increased
from roughly 37 microseconds to 1.3–1.6 milliseconds, and weak-tree workloads from roughly
45 microseconds to 650 microseconds. The baseline used inferred regions for these graphs;
cycle-managed types are now excluded until equivalent observable cleanup is proven.
Direct zero-count teardown reduces collector scratch overhead but does not recover bulk
region reclamation. Recovering this performance needs a proved region/collector interaction;
it remains follow-up work, rather than a claimed acceptance success.

The [final alternating runtime samples](runtime-cycle-measurements.json) include three
executions per version, each with three internal passes. Absolute timings varied from the
earlier batch: tree medians were 68.9 microseconds before and 2498.7 microseconds afterward;
weak-tree medians were 94.5 and 914.6 microseconds. Numeric kernel variation between batches
precludes claiming a speedup there. The graph-performance regression persists in both batches.

The final native corpus passed all 623 cases. Native/WASM parity passed 610 cases with
13 intentional native-host or pointer-width skips. Workspace build, strict Clippy,
workspace tests, runtime layering, repeated cycle-artifact determinism, and the relevant
ignored DAP, generator, PGO, WASM backend-level and packaging checks passed locally.
Worker, cycle and callback harnesses passed ThreadSanitizer and separate ASan/UBSan
runs on macOS. The automatic Linux sanitizer lane is configured but requires CI execution.
Eight selected cycle, destructor, weak and unowned fixtures also passed native/WASM parity
in both Debug with `-g` and Release after the final runtime changes.

References: [LLVM llc object output](https://www.llvm.org/docs/CommandGuide/llc.html),
[Bacon and Rajan, Concurrent Cycle Collection in Reference Counted Systems](https://pages.cs.wisc.edu/~cymen/misc/interests/Bacon01Concurrent.pdf).
