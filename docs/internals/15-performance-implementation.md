# Performance implementation campaigns

This file tracks implementation of the long-term performance plan. It is an implementation
record, not a claim that all five milestones have shipped. Dream syntax, sink-default ownership,
UTF-16 strings, monomorphization keys, and the C guest runtime remain the architectural contracts.

## First campaign: profiling fidelity and collection waste

Targeted costs are debugger-oriented MIR inlining suppression during profiling, one-element
zero-value buffers, repeated bulk-growth relocation, update-triggered hash-table growth,
tombstone-driven expansion, repeated insertion hashing, and per-object publication bookkeeping.

Implemented:

- `--release --perf-symbols` emits optimized source/function information without debugger local
  preservation. Ordinary `-g` keeps its debugging policy. Compiler and CLI link caches distinguish
  these settings. The profiling helper uses the new mode.
- A typed, compiler-internal zero storage value uses actual scalar/reference/inline layouts.
  `Buffer.zero` is internal to `system.core`; `CollectionStorage.zero` is internal to
  `system.collections`. Neither is a source-language default constructor. The analyzer also
  rejects user-defined uses of the storage intrinsic outside trusted stdlib implementations.
- Intrinsics respect declared member visibility before specialized generic dispatch. Ordinary
  intrinsic calls preserve visibility in the function table. Public stdlib intrinsics declare
  `public` explicitly; the parser preserves visibility on extern declarations and permits public
  intrinsic imports while retaining the host extern/export conflict.
- List removal/clearing, queue/priority-queue removal, collection queries, and Map/Set/SortedMap
  default storage no longer obtain zero values from one-element allocations.
- List bulk append and reserve calculate capacity once. Span ownership preserves self-append and
  overlapping sources across relocated backing storage.
- Map/Set avoid growth for existing entries and reuse the hash computed by insertion.
  Same-capacity compaction and tombstone-tail reclamation were rejected after the initial
  20-round run showed a supported 23–25% churn regression. Threshold/probe work remains pending.
- Publication uses stable blocks of traversal entries with uthash visitation. Storage is freed
  at the end of each traversal; cycles, shared-root revisitation, heap validation, and nested
  reentrant traversal remain intact. This batches allocations; cross-call reuse remains pending.
- Frame allocation budgets include headers/alignment. Debug tracing records pass elapsed time,
  statement/local/block counts, useful changes, and frame-placement acceptance/rejection reasons.
  This is the beginning of optimization accounting, not a complete effect/rejection taxonomy.

Correctness investigation found two prerequisite bugs: inline unions could also be marked as
pointer-niche unions, and managed value-array overwrites performed byte copies without releasing
old nested references. Inline unions now retain their full layout. Value-element stores snapshot
and retain borrowed sources before dropping the old occupant, so overlapping copies survive
cleanup callbacks. Fresh producers transfer their owned nested references.

`tests/bench/collections.dream` covers bulk append at 8/256/8192/131072/1048576 elements, reserve, managed pop and
clear, and update/deletion churn. The benchmark default selects Dream alone. Reference/current
arms can each use a preserved matching C runtime via `--baseline-runtime`/`--current-runtime`.
Optional existing external arms remain explicitly selectable. Reference format v5 records the
selected fixture's actual rows and dependencies; the default regression threshold is 10%.

## Evidence and acceptance

Keep timing independent of counters, correctness, sanitizers, and compilation. Preserve compiler,
runtime, fixture, executable checksums, configuration, and raw samples. The initial local reference
is under `target/perf-plan/baseline`; it is not a portable CI baseline. Local macOS arm64 evidence
does not establish x86-64, Windows, Linux, or dedicated-runner performance acceptance.

Required gates are workspace build, Clippy with warnings denied, workspace tests, full native
probe, deterministic emission, relevant wasm/Node parity, runtime layering, and affected
sanitizer tests. Campaign timing requires 20 alternating rounds, 95% confidence intervals, and
no supported execution regression above 10%. Inconclusive results stay inconclusive.

## Text campaign

Unescaped JSON strings allocate owned payloads directly, so later mutation of the input does not
change the parsed value. Quote/backslash boundaries use the existing UTF-16 vector scanning
pattern. Escaped and truncated payloads retain the parser's existing fallback behavior.
`StringBuilder.append_double` writes the existing formatter's ASCII units into the builder;
it no longer creates a temporary guest string. The formatter now terminates its NaN/infinity
branches, which previously left their C string terminator unspecified.

Native string output converts through a bounded 1024-byte stack buffer. A stream lock spans the
whole operation, and it flushes once. Existing C-string embedded-zero truncation and lone
surrogate encoding remain intact; panic writes retain their separate replacement semantics.
Native stream tests verify chunk boundaries, Unicode, stream selection, and allocation avoidance.

The local 20-round text run passes all five regression gates. Separate counters show 20000 fewer
builder allocations per pass and one fewer allocation per unescaped JSON parse. See
`tests/bench/results/performance-2026-10-12-text.md` and JSON. The recorded version's escaped
256-unit workload gained one allocation per parse. A subsequent capacity fix reserves for the
remaining input to avoid that growth; the predefined follow-up passes the 10% gate on all five workloads and restores escaped-input allocation counts. See `tests/bench/results/performance-2026-10-12-text-reserve.md`.

## Native heap teardown campaign (validation in progress)

Worker thread exit and embedding thread detach retire native heap state. Reusable class blocks
return to the process allocator and private slow allocation borrows them in batches. A separate
atomic availability hint avoids locking when the process class list is empty. Unused current
arena tails and their counter records can be adopted by a later thread. Retired counter totals
are aggregated under the heap lock; concurrent diagnostics cannot observe removed records.
Live published objects keep their mapping and survive the allocating thread's exit.

The injected-platform harness covers 64 sequential thread exits with a live shared object and
verifies bounded mapping/record growth for that workload. Native AddressSanitizer/UBSan and
concurrent counter checks pass. This does not reclaim mappings, recover every abandoned tail on
arena rollover, or establish an OS-retention budget. Those require slab metadata and the mapping
release ABI. The initial collection timing had two inconclusive rows. The predefined 20-round
follow-up passes the 10% hard gate on all rows, with a supported roughly 5% managed-pop slowdown.
The tradeoff is recorded in `tests/bench/results/performance-2026-10-12-heap-thread.md`.

## Publication and ready queue storage (validation in progress)

Native publication returns traversal blocks to a process cache bounded at 16 blocks of at most
256 entries each. Reentrant traversals own disjoint active blocks. Uthash tables are still created
per traversal; this removes entry-block allocation from recurring graphs within the cache bound,
not all hash-table bookkeeping allocation. The native stress test covers repeated cycles,
modified shared roots, nested traversal, diamonds and million-node graphs under sanitizers.

Ready queues retain 64 TLS entries and obtain overflow entries in blocks of 64. Drained run loops
return blocks to a process cache bounded at eight blocks; larger bursts release excess storage.
The `ready_alloc` counter counts actual block allocations. The native harness covers FIFO order,
64-entry boundaries, 513/1024/4096-entry bursts, repeated sessions and concurrent schedulers.
An end-to-end cancellation case checks removal from an overflowing ready queue.

Wasm returns these blocks to its guest heap instead of retaining a cache in mutable C module
globals: shared worker instantiation replays data segments. Portable batching remains enabled.
Scheduler timing has three inconclusive small-burst rows in its initial run; a predefined
follow-up waits for compiler processes to become idle and doubles measured passes. All rows pass the 10% gate; recurring 513/4096-entry bursts eliminate measured overflow block allocations. See `tests/bench/results/performance-2026-10-12-ready.md`.

## Function pass scheduling

A pass that found no changes can skip its next invocation if its function input is identical.
Any successful transformation increments the function revision and invalidates those decisions.
Successful passes remain eligible for another round, because they need not be idempotent.
Dumping still records every configured stage. An independent synthetic pipeline checks both
skipping and revisiting a previously unsuccessful pass after a new opportunity appears.
This is function-local scheduling; revisioned call-graph effect summaries remain pending.

## Runtime startup reachability

A compiler-internal requirements record is derived after MIR pruning. It selects argument/path
capture and callback-owner attachment/cleanup. Unused imports do not retain these startup calls.
Reachable process argument/path APIs preserve capture. Reachable async code, callback registration intrinsics and foreign/host
imports retain callback services conservatively. Closure allocation/accessors and callback
registration are classified in the intrinsic registry; opaque intrinsic effects retain startup
and cleanup services conservatively. Exported functions and C imports preserve the
embedding contract, including process capture; native libraries preserve its export roots.
Ordinary executables no longer preserve embedding exports unconditionally during internalization.
Argument capture itself no longer attaches a callback owner. Final deferred cleanup is selected from actual `defer` use or opaque embedding needs, before the leak report; the report no longer roots an unconditional drain. Leak reporting remains reachable,
including environment-controlled Release reporting. Debug tracing prints the requirement flags. Nine integration checks cover unused imports, arguments/path queries, deferred cleanup, C imports, local callbacks, opaque intrinsics and wasm manifests.
The recorded 20-round follow-up passes the 10% gate on all four launch workloads, without a supported launch speedup. Empty-program symbols fall from 108 to 19 and its TLS sections disappear. See `tests/bench/results/performance-2026-10-12-runtime-reachability.md`. This is the first startup/root selection step, not full runtime-unit dependency selection.

## Inline value observers (correctness prerequisite)

Weak/unowned fields of inline value structs must survive copying and relocation. Registering a
field address in a temporary constructor box left observer entries pointing into freed storage.
Inline observed fields now own stable ARC observer boxes; copies retain them and overwrites drop
old boxes only at their last owner. Class fields keep their existing observation protocol.
Array clearing and nested value cleanup use the same observer ownership rules. DWARF views and
LLDB formatting decode these boxes. Native and Node cases cover copied weak/unowned values,
self-assignment, List relocation with a live view, target death and dead-unowned traps; native
AddressSanitizer checks pass. Runtime bitcode/signature cache identities include the compiler's
embedded header declaration inventory, preventing stale anchors across compiler revisions.

Borrowed function-instance keys also avoid allocating temporary type-argument vectors in escape,
ARC summary and inlining lookups. Hash/equivalence tests preserve exact `(DefId, args)` identity.
Compile-cost and full-platform acceptance remain pending. The copied-observer DAP check passes, together with the existing string/array/union and async debugger checks.

## Timer and foreign-completion blocks

Timer and foreign-completion entries use separate 64-node blocks. Native caches retain at most
8 spare blocks per pool when idle. Wasm timer blocks return to the guest heap. Allocation cursors
initialize entries lazily, so small bursts do not rebuild all 64 slots. The last active entry
retires its pool's blocks; a current expiry/cancellation cannot invalidate another active node.
Raw allocation, free and panic hooks run outside the heap lock. Foreign pool access uses the
heap lock, while completion claiming and the existing scheduler queue state machine are unchanged.

The first timer fixture awaited lazy futures sequentially. The final fixture starts every future
before awaiting, spanning active bursts of 1/8/64/513/4096. All five 20-round rows improve with
95% confidence and pass the 10% gate. Separate counters show 5000→0, 8000→0, 6400→0, 10260→20,
and 12288→168 raw timer allocations per measured pass, with unchanged guest/ARC events.
All five application rows also pass. Native/Node cancellation, foreign completion lifecycle,
and concurrent pool sanitizer checks pass. See `tests/bench/results/performance-2026-10-12-scheduler-nodes.md`.
Scheduler-owned foreign synchronization and indexed stable deadlines are still pending.

## Heap-free diagnostic roots

A conservative scalar-only MIR proof lets exit reporting emit known zero counts without retaining
heap readers and enumeration. Managed or aggregate storage, entry arguments, async, opaque calls,
FFI, exports and numeric formatting that allocates retain full reporting. The internal reporting
ABI carries this requirement; all implementations and call sites migrate together. Debug and
environment-enabled Release reports preserve their output. Empty-program symbols fall from 19
to 11 and text from 1,628 to 104 bytes. Launch timing remains pending; see
`tests/bench/results/performance-2026-10-12-empty-heap.md`.

## Progress checkpoint validation

The current checkpoint passes workspace build, workspace Clippy with `-D warnings`, workspace
tests run serially, and the full native golden probe (679/679). Focused checks cover intrinsic
visibility, profiling fidelity, runtime requirements, leak reporting, native/Node cancellation,
observer copies, pool sanitizers and debugger views. Performance acceptance currently uses local
Apple arm64 hardware. The user will test Windows later; native x86-64 hardware performance and
complete affected-platform acceptance remain outstanding. Preserved local reference binaries and
runtime snapshots live under ignored `target/perf-plan/`; checked-in campaign records retain
measurement metadata and samples. The latest heap-free launch timing is pending at this checkpoint.

## Remaining milestones

1. Baseline: broaden application/startup/retention/compile workloads; complete runtime memory
   accounting and stable optimization rejection reasons; reproduce previous controlled results.
2. Everyday work: prove backing-storage transfer for `take_array`, isolate realloc behavior,
   audit rehash ownership, extend text/search improvements, and finish publication-storage
   acceptance across affected platforms.
3. Runtime: reachable-service requirements, feature-driven initialization/export roots, complete
   native heap-thread reclamation, slab reclamation and mapping-release ABI migration, scheduler-owned foreign synchronization, indexed stable timers and job reuse.
4. Compiler proofs: revisioned effect summaries, allocation-generation precision, broader
   borrowing/SROA/regions, suspension-aware ARC, reducible-loop field caching, and affine bounds
   proofs with independent verifier coverage.
5. Scaling: deterministic benefit/cost inlining, closure and receiver propagation, bounded
   specialization/PGO, generic instance accounting, and change-driven optimization worklists.

Do not enable storage transfer through shared buffers merely because one local owns a token.
Do not add default busy spinning, automatic task migration, tracing GC, dynamic stack allocation,
or unproven LLVM alias/overflow/floating-point metadata to close these milestones.
