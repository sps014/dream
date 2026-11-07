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
registration, component metadata, candidate queue, or strong-edge barrier. Object headers
are 8 bytes (`[tag][rc]`); size classes live in slab metadata. Private references use
the local count fast path; published references use atomic counts.

The cycle analysis (`dream-sema` `analyzer/cycles/`) prefilters on the strong type graph,
then walks function bodies flow-sensitively with bottom-up call summaries and rejects
stores that can close a strong cycle. Arrays, tuples, containers, erased `object`,
interface and closure edges and active union payloads participate. Weak and unowned edges
do not. `@allow_cycle` accepts cycles through objects of the annotated class; it does not
enable collection or change emitted ARC operations. Infinitely sized inline
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

### Compile-speed profile against Go

The [second measurement set](build-profile-measurements-v2.json) profiles every build
scenario and compares it with `go build` (Go 1.27.3) on programs of the same shape, on
macOS arm64 with the pinned LLVM 22.1.8. `scripts/bench.py build --workload <name>` and
`scripts/bench.py go --workload <name>` reproduce it. The workloads are:

- `hello`: one class and `main`.
- `gen200` and `gen2000`: 200 or 2000 functions, each with one allocation, a loop and a
  branch (3,000 and 30,000 lines).
- `stdlib`: `@json` serialization, a regex, `List` and `Map`.

Medians of five repetitions, in seconds. Dream uses the system `cc` through the tracing
wrapper, which adds about 30 ms per subprocess. Go's cold build includes compiling its
standard library, the counterpart of Dream's runtime C build.

| Workload | Dream Debug cold | Dream Debug edit | Dream unchanged | Go cold | Go edit | Go unchanged |
|---|---|---|---|---|---|---|
| hello | 4.33 | 1.20 | 0.13–0.27 | 2.14 | 0.15–0.29 | 0.06–0.13 |
| gen200 | 5.50 | 1.65 | 0.21 | 2.40 | 0.16 | 0.07 |
| gen2000 | 8.83 | 5.65 | 0.16 | 2.34 | 0.37 | 0.08 |
| stdlib | 26.48 | 3.16 | 0.31 | 2.37 | 0.21 | 0.09 |

Release edits cost about the same as Debug edits plus 0.1–1.3 s of whole-program
`opt`/`llc`: 1.32 s for `hello`, 6.95 s for `gen2000` and 4.21 s for `stdlib`. Wasm Debug
edits are within 0.8 s of native. Release `stdlib` is the exception: `wasm-opt` adds 5.4 s
to each wasm edit, for 12.6 s in total. With the default Zig linker driver, unchanged builds
take 0.6–0.7 s and edits take up to 2.1 s longer than with the system `cc`.

#### Where the time goes

Phase spans (`compile_phase`) and 1 ms `sample` call trees attribute the cost as follows,
largest first:

1. **Quadratic source positions.** `LineText::get_point` walks every line from the start of
   the file for each token, so lexing is O(tokens × lines). The user file is lexed twice:
   once by the parser and again by `module_graph::hash::prepare_module`. In the `gen2000`
   Debug edit, 3.6 s of 5.6 s is `Lexer::lex_all`, 1.5 s of it in `get_point` itself. Line
   widths are also counted in characters while lexer spans are byte offsets, so columns are
   wrong after non-ASCII text.
2. **Re-hashing tools on every build.** The early cache key (`request_key`) BLAKE3-hashes
   the compiler, `opt`, `llc`, `llvm-link`, `llvm-dis`, `clang`, the C driver and every host
   capability library. `tool_identity` memoizes only within one process. This is 0.28–0.47 s
   of every build, 77% of an unchanged `hello` build, and about 0.6 s with the 185 MB Zig
   driver.
3. **Generator pass work without matching generators.** Generator discovery, building the
   module graph a second time, and parsing the std generator sources cost 0.12–0.24 s per
   `hello` build. With `@json` in use, the pass takes 1.7 s per edit. Of that,
   `generate::body::lower` (lowering referenced function bodies into the snapshot) is 0.7 s
   and `analyze_for_generators` is 0.37 s, even when every generator executable is cached.
4. **Re-parsing the prelude.** Bootstrap and imported packages are lexed and parsed on every
   build: 0.15–0.2 s, or 0.4 s with `json`/`text` imported. Monomorphization adds a near
   constant 0.12 s (0.5 s with `stdlib`).
5. **Cold runtime and generator builds.** A cold Debug build compiles 38–48 runtime C units
   (about 2.5 s) and builds runtime signatures (1.1–1.5 s). Each std generator executable is
   compiled and linked through the full Release bitcode path, so a cold `stdlib` build takes
   26 s, 18 s of it in generator links.
6. **Program LLVM work.** Debug `llc` plus the final link is 0.08–0.5 s per edit, and
   in-process link bookkeeping adds about 0.1 s. This is close to Go's link cost and is not
   the bottleneck. A Cranelift Debug backend would remove at most this slice.

#### Fix candidates

Estimated savings are taken from the measured phase and sample shares; each needs its own
before/after measurement.

| Candidate | Saves (Debug edit) | Saves (unchanged) | Effort | Risk |
|---|---|---|---|---|
| Cache the generator analysis (`analyze_for_generators`, about 0.35 s with `@json`) across edits that leave the generator inputs unchanged | 0.35 s with `@json` | — | Medium | Medium: incremental-generator correctness |
| Cache the parsed prelude per package (`prelude_parse` is about 0.1 s for `hello`) | 0.1–0.4 s | — | Large | Medium: arena AST lifetime |
| Ship prebuilt runtime objects, runtime signatures and std generator executables with the toolchain | — | — | Medium | Low; cold Debug drops from 4–26 s to about the edit time |
| Cranelift Debug backend | at most 0.1–0.5 s | — | Large | High |

Reaching Go's 0.15–0.37 s edits needs the generator and prelude caches.

#### Applied fixes

- `LineText` keeps a line-start table and resolves a span by binary search, so diagnostics
  and debug locations no longer rescan the source from the start for every span. Columns
  count characters, not bytes.
- Tool identities (the compiler, LLVM tools, the linker driver, host libraries, and the
  runtime-signature tools) persist in `<toolchain prefix>/cache/tool-identity`, keyed by the
  same device, inode, size, mtime and ctime stamp the in-process cache trusts. A file whose
  ctime is within two seconds of hashing is not persisted, because a same-tick rewrite would
  keep its stamp.
- Building the module graph once is not possible as stated: the generator pass builds its
  graph before generation, and the final graph includes generated files. The second lex in
  module-graph hashing stays; it is linear in the source size.

Plain wall times, native Debug, median of 3–5 runs, before and after these fixes:

| Workload | Linker | Edited before | Edited after | Unchanged before | Unchanged after |
|---|---|---:|---:|---:|---:|
| `hello` | Zig | 1.71 s | 0.81 s | 0.74 s | 0.06 s |
| `gen2000` | Zig | 7.76 s | 1.89 s | 0.69 s | 0.06 s |
| `hello` | system `cc` | 1.20 s | 1.21 s | 0.27 s | 0.09 s |
| `gen2000` | system `cc` | 5.65 s | 2.1–3.3 s | 0.16 s | 0.11 s |
| `stdlib` | system `cc` | 3.16 s | 3.23 s | 0.31 s | 0.07–0.41 s |

Unchanged builds now match Go's 0.06–0.09 s. The `stdlib` edit is still dominated by the
generator pass.

A second round removed per-process work that did not depend on the edit:

- The embedded generator packages (`system.json.derive`, `system.gpu.shaders`, …) are parsed
  only when a loaded package registers them, once per process. A program that loads none of
  their owners skips them; `hello`'s generator pass went from 73–83 ms to about 2 ms.
- The snapshot's referenced-declaration closure remembers ids that resolved to nothing or to
  an already-recorded declaration. Member references such as `Type.field` used to re-lower
  every method body of `Type` for each occurrence. Snapshot building with `@json` went from
  0.92 s to 0.15 s; the emitted snapshots and span maps are byte-identical on the JSON,
  validation and GPU workloads.
- Module content and interface hashes are computed on demand (`ModuleGraph::module_hashes`).
  Only the build stamp reads them, so the generator pass's graph no longer pays about 70 ms
  for an AST print and re-lex of every module.
- The `opt --version` check is remembered per `opt` content identity, and the runtime-source
  fingerprints (the build stamp's and the runtime bitcode stamp's, which include `clang`)
  persist under the files' stat stamps. Spawning `opt` cost 30–280 ms and hashing `clang`
  about 100 ms on every build. Runtime unit and object cache keys use the persisted `clang`
  identity as well.

The bimodal `llc` (14 ms or 275 ms) and `cc` link (55 ms or 380 ms) times on macOS follow
`syspolicyd` load, not the compiler.

The same machine under the same background load, alternating the two compilers, median of 5:

| Workload | Edited before | Edited after | Unchanged before | Unchanged after |
|---|---:|---:|---:|---:|
| `hello` | 1.05 s | 0.74 s | 0.06 s | 0.02 s |
| `stdlib` | 3.68 s | 2.57 s | 0.38 s | 0.05 s |

Every compile also caps the shared caches under the toolchain prefix. Generator results stay
within 256 MB, generator executables within 1 GB, runtime objects within 512 MB, stdlib
source snapshots within 32 MB and remembered tool identities within 64 MB. The least recently used entries are removed; anything modified
in the last 15 minutes is kept, so a compile does not delete an object another link still has
open. A cache hit refreshes the entry's modification time. Each directory is measured at most
once a minute across processes; a `.budget-checked` marker records the last check.

A third round stopped analyzing standard-library bodies that the program cannot reach. The
driver enables `Analyzer::with_lazy_library_bodies`. Std free functions and std struct and
extension methods are then deferred, and the instantiation fixpoint analyzes only those that
already emitted HIR reaches. The edges match MIR dead-code pruning:

- direct callees and function references;
- the destructor, `to_string` and `hash_code` of every type a reachable body or global
  mentions, including nested type arguments and field types;
- the protocol methods of every primitive;
- for each interface slot that is called, the method filling it in every implementing type.

Exported functions stay roots. Once a compile has reported an error, every deferred body is
analyzed, so diagnostics in library bodies still appear. A generic value struct named only by
a field (such as `ReadOnlySpan<char>` inside `StringLines`) is instantiated directly, because
every registered struct gets a layout. The LSP, generator analysis and the `dream-sema` tests
keep eager analysis. For `hello`, `sema` went from 147 ms to 25 ms and monomorphization from
112 ms to 2 ms. Later phases gain as well, because far less unreachable code reaches lowering
and the MIR passes. Alternating the two compilers, median of 8:

| Workload | Edited before | Edited after | Unchanged before | Unchanged after |
|---|---:|---:|---:|---:|
| `hello` | 0.55 s | 0.41 s | 0.01 s | 0.01 s |
| `stdlib` | 1.96 s | 1.13 s | 0.01 s | 0.01 s |

Any of the C compiler variables in `ToolchainConfig::compiler_environment` (`CPATH`,
`LIBRARY_PATH`, …) disables the build caches, so a benchmark harness must clear them. Homebrew
Python exports both.

A fourth round removed the Zig linker driver and stopped re-running generator analysis when
the generator's inputs are unchanged.

Native, cross, PGO, sanitizer and package C/C++ links all go through the pinned clang.
ELF and Mach-O use mold (`ld64.mold` on Apple, selected with `--ld-path`); COFF uses the
pinned LLVM `lld-link` (MSVC) or `ld.lld` (MinGW) via `-fuse-ld=lld`. wasm32 still calls
`wasm-ld` directly. There is no system-compiler fallback. A cross link needs `DREAM_SYSROOT`,
or `SDKROOT` for Apple. Link lines pass `--build-id=none` on ELF, `/Brepro` on MSVC and
`--no-insert-timestamp` on MinGW, so two links of the same objects to the same path are
byte-identical (`native_binary_is_deterministic`). mold is pinned at commit `331ac66` and
built from source into the toolchain (`scripts/fetch-dev-llvm.sh`); it is not part of the
compiler crate build. On this machine mold's Mach-O link was faster than `ld64.lld` and the
system `ld` (about 25 ms versus about 38 ms on a replayed hello link).

The generator pass used to run `analyze_for_generators`, expand the snapshot closure and
lower referenced bodies on every compile that matched a trigger, then often replay a cached
generator executable result. The new program cache (`src/driver/generate/result_cache.rs`)
sits in front of that. Its key is the compiler identity, the target layout, the generator
executable identities, the fingerprints of the tools those generators run, and the syntactic trigger set: each matched call's enclosing
declaration, each attributed declaration, and each syntax block. The stored entry is the
generated files plus the module interface hashes and the text hashes of the declarations the
snapshot closure named. A hit merges those files and skips semantic analysis, snapshot
expansion and body lowering. Editing an unrelated function body hits. Editing a trigger
call, an attributed declaration, or a declaration the generator read misses. The snapshot's
symbol list is trimmed to that closure, so an unrelated generic instantiation does not
change what the generator would have seen. `DREAM_GENERATOR_CACHE=0` disables the cache.
The entry lives under the toolchain generator cache and shares the 256 MB result budget.

`scripts/bench.py build` on this machine, release host compiler, native Debug, `--plain`,
median of 3. The harness gives each repetition a fresh `DREAM_PREFIX`, and its edit changes
a literal inside the program, so a `stdlib` edit still misses the program cache and pays
the full generator pass:

| Workload | Edited | Unchanged |
|---|---:|---:|
| `hello` | 0.38 s | 0.022 s |
| `stdlib` | 2.03 s | 0.026 s |

The third-round medians on the same shape were 0.41 s / 0.01 s for `hello` and 1.13 s /
0.01 s for `stdlib`. Unchanged builds stay in the same band. The `stdlib` edit is slower
here because the changed literal is a generator input, and the cache records its
dependencies on the miss path. An edit outside that closure reuses the generated files;
`unrelated_function_edit_reuses_the_program_cache` covers that case.

#### Toolchain failures found during measurement

- With the default Zig 0.16 linker driver, native Release links and every source-generator
  executable fail with `duplicate symbol definition: _main`, so programs that use `@json`
  do not build with the default driver on macOS. The measurements above use the system `cc`.
- An executable `DREAM_WASM_OPT` override that matches the install layout
  (`…/binaryen-133/bin/wasm-opt`) resolves to `…/binaryen-133/wasm-opt`: `from_override`
  records the install root but marks it as not owned, so `LocatedTool::executable` keeps only
  the file name.
- The system `ld` (ld-27037) rejects the locally built `libdream_host_net.dylib` with
  "mis-aligned LINKEDIT string pool", so the `webapi` sample could not be measured with the
  system `cc`. The misalignment comes from the release profile's default debug-info `strip`.
  Building with `CARGO_PROFILE_RELEASE_STRIP=none` produces a library that links.

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
