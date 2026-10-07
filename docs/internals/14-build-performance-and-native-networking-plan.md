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
release operations do not allocate collector state or acquire its gate. Zero-count cycle-capable
objects use direct iterative teardown without trial counts or ordering scratch: no incoming
strong edge can remain, so the object cannot belong to a cycle.

Cycle reference operations and strong-edge mutations use a runtime gate. Doomed objects are
claimed under the gate; user destructors run after releasing it. Weak and unowned handles are
invalidated before finalization. Finalizers run once in allocation order while peer storage
and edges remain readable. Edges are cleared and storage reclaimed afterward. Temporary
finalizer reads cannot revive ownership; publication and mutation of doomed objects are rejected.
Reentrant destruction joins the current drain. Explicit `defer` postpones collection, and its
final drain must precede leak reporting.

Inferred `UniqueRegion` remains a Release optimization for proven nonescaping, destructor-free
graphs. Collector-managed objects are excluded pending proof of equivalent cleanup. Future
explicit graph ownership can define a lifetime for deliberately shared cyclic graphs; no new
syntax or API is introduced here. Determinism concerns the same synchronized execution;
concurrent scheduling has no added global ordering guarantee.

## Validation

Measure cold runtime builds, warm edited builds, unchanged builds, compiler rebuilds, and
Release builds separately. Keep repeated raw timings, phase costs, memory and subprocess
counts. Architectural changes alone do not establish a speedup. Test native/WASM cleanup
parity and byte-identical repeated artifacts; preserve Release runtime benchmarks. Required
gates are workspace build, strict Clippy, workspace tests, the full native corpus, parity,
runtime layering checks, relevant ignored DAP/toolchain tests, and Linux sanitizer CI.

### Measured results and remaining performance cost

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
