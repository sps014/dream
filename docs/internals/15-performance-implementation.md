# Long-term performance implementation status

This is an implementation ledger, not a portable performance claim or a declaration that the
whole plan has shipped. Dream keeps its syntax, sink-default ownership, UTF-16 strings,
monomorphization keys and C guest runtime. Machine-specific samples and runner/tool identities
stay under ignored `tests/bench/out/` directories; machine-specific result files are not committed.

## Implemented mechanisms

### Profiling and measurement

`--release --perf-symbols` emits source/function information while retaining ordinary Release
MIR optimization. Debugger local preservation remains separate and ordinary `-g` keeps its
policy. Compiler/link caches distinguish the settings; the profiling helper uses performance
symbols. Dream-only comparisons are the default. Existing C# arms remain explicitly selectable.

The harness preserves matching compiler/runtime snapshots, fixture bytes, checksums, tool and
runner identities, raw samples and process RSS. Timing is separate from counters and sanitizers.
Campaigns use alternating execution, discarded warmups and process-level confidence intervals.
The initial regression budget is 10%; inconclusive results remain inconclusive. See
`tests/bench/BASELINE.md` for the protocol. New suites cover collections, text, long-needle
search, scheduler overflow, timers and repeated worker sessions. Startup and compiler-build
measurements use their own entry points.

### Collections and typed storage

Compiler-internal typed zero storage follows existing buffer initialization layouts without
calling constructors or adding source-language default construction. `Buffer.zero` and
`CollectionStorage.zero` have their declared internal visibility. Intrinsic specialization checks
visibility before generic dispatch; public stdlib intrinsics declare `public` explicitly.
Analyzer validation restricts the storage intrinsic to trusted stdlib implementations.

Collection removal/clearing and container default storage avoid one-element temporary buffers.
List reserve/bulk append calculates capacity once and preserves self-append and overlapping spans
through relocation. Map/Set updates avoid insertion growth and reuse the computed hash.
A rejected tombstone-compaction experiment is absent from the implementation; its evidence
remains in local JSON. Inline unions keep their full layout. Managed value-array overwrite retains
borrowed sources before dropping old occupants, preserving overlapping copies and cleanup callbacks.

PriorityQueue uses a four-child heap with comparisons completed before opening a hole. Typed
`ArraySlotSwap` and `ArraySlotTake` MIR operations preserve or transfer slot ownership without
transient reference counts; their LLVM lowering keeps index checks and supports managed value
fields. `List.reverse` uses the permutation operation. Reentrant queue mutation from a comparator
is rejected before mutation, while callbacks can read the queue and change unrelated state.
Option/Result success extraction uses internal zero storage for unreachable fallback values,
rather than allocating a one-element array on every unwrap.

Before ARC insertion, `function-targets` resolves non-capturing boxes and constructor-initialized
function fields whose live writes agree. Field inference rejects exports, opaque escapes, raw
writes, conflicting constructors and casts across type identities; captured callbacks stay
indirect. Resolved direct calls keep the function-value borrowed ABI and may inline even when
the target remains address-taken. Dump this stage with `--emit-mir=after:function-targets`.

### Text

Unescaped JSON strings construct owned payloads directly. Quote/backslash scanning uses the
existing vector boundary scan; escaped strings reserve builder capacity. Builder floating-point
formatting avoids a temporary guest string, and NaN/infinity formatting terminates correctly.
Native output uses bounded conversion storage, one operation-wide stream lock and one flush,
preserving existing embedded-zero, lone-surrogate and panic behavior.

Forward UTF-16 long-needle search adapts musl's constant-space Two-Way implementation with its
MIT notice. Short needles retain their path; first-unit scanning and immediate equality protect
early/sparse matches. Tests cover explicit lengths, embedded zero units, surrogates, mutation,
repetitive inputs and boundaries. Reverse search remains unchanged.

### Heap and publication

Raw arenas cache the active cursor and end address, with chunk growth out of line.
Reset rewinds the first chunk in constant time; later chunks are reset when revisited.
String-backed span reads use an internal unchecked unit accessor after the logical span
check, avoiding a duplicate backing-string check without changing public bounds behavior.

Native thread exit and embedding detach return reusable class blocks to the process allocator.
Private refill adopts them in batches; an atomic availability hint avoids empty-list locking.
Unused current arena tails can be adopted by later threads. Counter records retire under the
heap lock with totals aggregated; live published objects survive their allocating thread's exit.
This does not reclaim OS mappings or recover every arena-rollover tail.

Publication uses uthash visitation and stable traversal blocks. Native scratch caches are bounded
and process-owned; active recursive traversals own disjoint blocks. Shared roots are revisited
because they can acquire private children. Heap validation, cycles and exact visitation remain.
Wasm scratch returns to the guest heap; per-traversal uthash bookkeeping still allocates.

### Scheduler and workers

Ready overflow, timers and foreign completions obtain entries in bounded reusable blocks.
Native idle caches are bounded; wasm returns storage to its guest heap. Cancellation removes raw
queue references before discharging scheduler ownership. Timer ordering still uses a linked list.

Each native worker session retains at most one payload-free spare Job. Atomic adoption gives
exclusive ownership; reply/completion/queue fields reset before reuse, and session destruction
frees the spare. Reuse preserves callback ownership and thread-local/session isolation.

Native scheduler completion queues, pending credits and wake/park predicates now share a
scheduler-owned mutex. Draining and callback wake-up do not take the global completion-claim lock.
Pending credit stays held through publication and enqueue, protecting the TLS scheduler lifetime.
Only claim-to-scheduler nested locking is permitted; guest cleanup runs after queue unlock.
Claiming, cancellation/status coordination and result publication still use the global claim lock.

Foreign work publishes its future and owned graph on the owner before host handoff. Publishing
only during completion permitted private ARC updates to overlap the shared transition. The
existing erased/interior-store publication gate is enabled before handoff. Owner-only intrinsic
timers keep their private allocation path. Independent tests check concurrent schedulers,
duplicate completion, cancellation, pending credits, owner-thread wake/resumption and balanced
shared scheduler tokens; they fail on the old locking and private-handoff paths.

### Compiler proofs and scheduling

`TaskScope.spawn_parallel` starts a structured CPU child through `Task.spawn`, using
the runtime's shared native thread or JS worker reuse. It joins submitted work on
cancellation. Explicit `spawn_on` pools additionally bound parallelism. Both CPU
entry points enforce the worker shared/move capture rules; local async `spawn` does not.

Loop-carried cursor proofs account for implicit cleanup through local value graphs, so
an unrelated module destructor alone does not disable a read-only traversal. Explicit
calls, mutation and destructor-bearing local cleanup retain their conservative checks.

Heap effects are shared by ARC insertion, held-owner snapshots, value borrowing and cleanup
proofs. Their monotone closure includes direct calls, fresh constructors and closed interface
targets; erased slots join all concrete instances. Destructor aggregation follows closure, and
unknown/missing bodies stay conservative. Owning cleanup and ownership observations are explicit
proof boundaries. Cursor inference rejects summarized overwrites, relevant destruction and heap
or externally aliased value-slot writes; fresh inline storage remains separate. Initial borrowed
expressions do not gain an owning token solely because their count is observed.

A deterministic dependency worklist closes these effects. Only growing summaries schedule users.
A full-rescan oracle checks generated graphs, recursion, unknown targets and generic instances;
scaling fixtures verify bounded evaluations and identical optimized IR. Parameter-position
read/write/escape/return-alias summaries and revisioned summary reuse remain pending.

Function passes skip a previously unsuccessful invocation only while the function revision is
unchanged. Successful transformations invalidate those decisions. This is function-local
scheduling, not a complete changed-function/call-graph optimization pipeline. Borrowed function
keys avoid temporary type-argument vector allocation without changing `(DefId, args)` identity.
Frame budgets include headers/alignment; tracing records useful changes and selected rejection
reasons. Full pass-cost and rejection accounting remains pending.

### Runtime roots and correctness prerequisites

Reachability requirements after MIR pruning select argument/path capture, callback attachment,
deferred cleanup and executable export roots. FFI, exports and libraries conservatively retain
their embedding contract. A scalar-only proof permits known-zero exit diagnostics without heap
readers; all reporting ABI implementations migrated together. Environment-controlled Release
leak reporting and Debug output remain available. Full runtime-unit dependency selection remains.

Copied/relocated inline weak and unowned fields own stable observer boxes; class fields retain
existing semantics. Cleanup, arrays, DWARF and LLDB views use that representation. Runtime cache
identities include embedded header declarations to prevent stale signatures across revisions.

## Validation status

The heap-effects/worklist checkpoint passes workspace build, strict workspace Clippy, workspace
tests, the full native golden corpus and selected Node parity checks. Standalone text, worker,
heap/publication and scheduler tests exercise applicable ASan/UBSan and TSan paths. The installed
scheduler-local locking and foreign-handoff changes pass workspace build, strict Clippy, workspace
tests, the full native probe (681 cases), five selected Node cases and the actual foreign-host
ThreadSanitizer integration. An initial eight-job probe had one generator compile timeout; the
focused case and full four-job rerun pass. Separate performance measurements remain local. Arm64 is the current hardware acceptance scope; Windows testing is deferred
at the user's request. No local result establishes native x86-64 or other-platform performance.

## Pending work, in delivery order

### Evidence and everyday allocation

- Expand feature-specific startup, working-set sweeps, publication graphs, foreign completions,
  value/closure/interface calls, generic scaling and realistic application workloads. Reproduce
  historical results with preserved fixtures and matching runtime/compiler artifacts in ignored output storage.
- Complete requested/class bytes, mapped/committed storage, per-thread/class free storage,
  large-block search, stranded storage and region-retention accounting. Separate object death,
  reusable storage and memory actually returned to the OS.
- Make rejection reasons stable across escape/publication, unknown effects, owner destruction,
  weak/unowned observation, suspension, aliasing, growth/frame budgets and iteration limits.
- Prove List backing-storage transfer for `take_array`, including live views and retained aliases;
  retain the correct copy path when transfer cannot be proven. Isolate realloc costs and managed
  element movement. Audit rehash ownership transfer before removing ARC traffic.
- Measure Map/Set probes, equality calls, live load/tombstones and rehash frequency. Tune growth
  and compaction with churn/working-set evidence before considering representation changes.
- Complete reverse search and comparison work, allocation-free text paths and concatenation
  fusion while preserving evaluation order, Unicode, mutation and snapshot semantics.

### Bounded, minimal runtime

- Add side metadata for slab lifetime. Account for live objects, TLS/process free-list references,
  cross-thread frees, published objects and arena tails before reclaiming a slab. Remove heap-map
  and debug-enumeration entries before releasing storage.
- Add mapping release to the platform ABI and migrate every native, wasm, injected-test and
  embedding implementation together. Bound retained idle slabs without slowing private allocation;
  wasm must return storage to its guest heap. Do not add a tracing collector.
- Replace the linear timer list with an indexed deadline heap with stable equal-deadline ordering,
  efficient cancellation, correct raw-frame lifetime and protected small-timer overhead.
- Prove completion/cancellation lifetime so result publication can leave the global claim critical
  section safely. Measure multiple schedulers and keep exactly-once claiming and owner resumption.
- Extend publication scratch reuse to remaining hash bookkeeping, preserving reentrancy and bounded
  retention. Remove heap validation only where provenance is actually proven.
- Improve larger recurring worker bursts and batching within current APIs. Preserve callback/TLS
  contracts; no default busy spin, automatic migration or unproven work stealing.
- Complete service-level requirements and runtime-unit dependency selection, explain retained
  services and implement affected Windows dead stripping. Keep opaque FFI and embedding conservative.

### Stronger compiler proofs

- Add parameter-position reads/writes, capture/escape, return aliasing, publication, destruction,
  observation, suspension, callbacks and raw exposure. Compute conservative component summaries
  with explicit function/module revisions and invalidation after transformations.
- Introduce allocation-generation identities so local reuse does not merge unrelated allocations;
  start intraprocedurally, then propagate through direct calls and closed interface sets.
- Broaden borrowing and held-owner snapshots with managed fields/returned aliases and observable
  cleanup proofs. Add suspension-aware ownership obligations before further async ARC elimination;
  cancellation must discharge each suspended owning slot exactly once.
- Expand scalar replacement/frame/region placement through branches, aliases, nested managed fields
  and proven non-retaining calls. Reuse frame slots only for verified disjoint lifetimes, budget
  aligned headers and keep recursion, publication, weak observation and suspension conservative.
- Generalize field caching to reducible multi-block loops, calls and early exits. Preserve zero-trip
  traps, owner lifetime, shared visibility, alias writes and live-outs. Extend affine/nested/span
  bounds proofs and guarded loops; emit no unproven alias, overflow or floating-point facts.

### Scalable Release optimization

- Build deterministic inlining benefit/cost decisions around constants, loops, wrapper flattening
  and allocation/ARC opportunities under function/module growth budgets. Keep address-taken and
  entry-point safety restrictions until their contracts are established.
- Resolve known closure/function/environment pairs, propagate exact receivers and add bounded
  constant specialization/PGO dispatch promotion. Track generic instances and duplicate machine
  code without changing monomorphization identity or introducing erased fallbacks.
- Audit remaining value copies/internal ABI only where measured; preserve clang-owned C shims.
- Measure pass time, allocations, IR growth and analysis rebuilds. Use changed-function/component
  worklists, shape-based skips and dependency scheduling rather than broad repeated rescans.
- Optimize runtime/tool signature-cache validation without weakening content correctness. Add only
  arena-safe immutable front-end caching and deterministic parallelism with shared mutation removed.

Each proof family needs independent verifier coverage and the alias/cleanup/trap/FFI/async matrix.
Each runtime phase needs thread exit, live published objects, cancellation, duplicate completion,
queue overflow, equal deadlines, allocation failure and embedding coverage. Required completion
checks remain workspace build, Clippy with warnings denied, workspace tests, full native probe,
determinism, relevant Node parity, layering and affected sanitizer/DAP/toolchain tests.
