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
and explicit strong-cycle ownership. Cleanup boundaries are established before optimization.
Immortal singletons preserve their reference-count sentinel, and remain
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

## ARC and cycle permissions

Both profiles use ordinary ARC. There is no automatic cycle collector, collector
registration, component metadata, candidate queue, or strong-edge barrier. Native object
headers remain 32 bytes; the former collector index is padding. Private references use
the local count fast path; published references use atomic counts.

The compiler follows strong field ownership after concrete type discovery. Classes that
can form cycles require `@allow_cycle`, including classes with owning erased `object`,
interface, or closure fields. Arrays, tuples, containers and active union payloads
participate. Weak and unowned edges do not. The annotation acknowledges possible leaks;
it does not enable collection or change emitted ARC operations. Infinitely sized inline
values remain invalid regardless of the annotation.

Use weak/unowned back-links or explicit teardown to break cycles. All-strong cycles
remain allocated; Debug leak diagnostics report their outstanding objects. There is no
cyclic finalizer ordering or special access to dying peers. Noncyclic finalizers run once
at zero count, before owned fields are cleared and storage is reclaimed. Explicit `defer`
postpones cleanup and drains before leak reporting. Resurrection remains forbidden.

A dedicated weak registry lock protects observer registration, slot snapshots and
observed-target zero-count claims. Reads acquire a temporary strong reference under that
lock. Weak reads become empty after destruction; unowned reads trap. Slots are invalidated
before user finalizers run, and user code never runs under the registry lock. Objects
without observers require no registry lookup or lock.

Exact strong-edge visitors remain for worker publication. Publication marks reachable
references shared for atomic ARC; application code must synchronize concurrent field
mutation. Async frame transfers retain moved ownership, explicit cleanup boundaries and
lazy environment snapshots without collector synchronization.

Release retains private regions for proven nonescaping, destructor-free allocations,
scalar replacement and ARC elision. Verified fresh field initializers can omit releases
of zeroed fields; callbacks and weak operations retain ordinary construction. Debug keeps
bounded optimization, ownership validation and leak reporting. Exactly Debug and Release
remain available.

## Release optimization mechanisms

Release span borrowing also admits fresh private string owners. Ownership dataflow proves
that the original owner remains alive at every view and derived-reference read, including
joins and back edges. Escape analysis includes the source fields of inline views, so an
escaping view or published source prevents this specialization. Ownership observations and
ordinary opaque calls retain the checked path. A bounds-check panic cannot access the private
source through its hook; the normal owner remains alive until abort. Debug retains the
ordinary validation path, and the optimization never postpones the original owner's cleanup.

Type metadata keeps separate exact traversal, finalization, edge clearing and storage
reclamation hooks. It has no collector flags or component identity.

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

### Historical automatic-collector checkpoint

The following results describe the earlier automatic-collector implementation, before
the Swift-style ARC migration above. They are retained as historical measurements and
are not validation results for the current implementation.

Release runtime performance was not fully preserved. Repeated microbenchmarks showed
numeric kernels close to baseline, but recursive tree allocation/reclamation increased
from roughly 37 microseconds to 1.3–1.6 milliseconds, and weak-tree workloads from roughly
45 microseconds to 650 microseconds. The baseline used inferred regions for these graphs;
cycle-managed types were excluded until equivalent observable cleanup is proven.
Direct zero-count teardown reduces collector scratch overhead but does not recover bulk
region reclamation. Recovering this performance needs a proved region/collector interaction;
it remained follow-up work at that checkpoint.

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
