# Native pointer migration: inventory and measurements

This engineering record tracks Phase 3.7. It does not establish that the migration or its
cross-platform execution gates are complete. The pre-migration artifacts remain
under `target/audit-p3-pointer-baseline/` locally; they are ignored build products.

## Representation inventory

The baseline native `dream_ptr` is `uintptr_t`; wasm32 uses `int32_t` linear-memory
offsets. `AbiTy::Ptr` and `MemTy::Ptr` currently lower both through `Lcx::h`, which
is an integer. `AbiTy::Word` is different: it represents `isize`/`usize` and must
remain an integer even when references become pointers.

Class instances, strings, arrays, boxed primitives/unions, closure environments,
future frames and the global environment are actual payload addresses. Their
native values, arguments, returns and stored slots should become LLVM `ptr`.
String slice owners and UTF-16 unit pointers are addresses too. Inline value
aggregates do not become references themselves: only their reference-bearing
fields change LLVM storage type, retaining `LayoutTable` offsets and alignment.

Nullable/niche references use pointer null; explicit union discriminants remain
integer tags. Interior `ref` arguments and weak registry destination slots are
addresses into live storage, not integer IDs. Weak target identity, lock registry
identity, publication worklists and atomic reference slots also contain addresses;
their identity comparisons become pointer comparisons and atomic slots must match
the pointer representation. Weak registry bucket hashing may explicitly convert
an address to `uintptr_t`, without changing the stored address or its lifetime.

Closure objects contain an environment address and an integer function-table
index. Interfaces combine object identity with dispatch tables: object/table
addresses are pointers, but type tags, function indices and selector IDs remain
integers. Async frames contain pointer-valued results, wakers and children beside
integer state/poll/kind words. Worker IDs and poll indices stay integers, whereas
worker environment/message handoffs contain actual references. Scalar async
results still need their typed representation rather than a blanket pointer cast.

`CPtr.raw` is an explicit `usize` raw-address boundary defined by Phase 3.5. Its
foreign-pointer box is a managed object pointer; its raw field stays an integer.
C marshalling converts that raw word explicitly when passing a C pointer. Genuine
foreign C pointer parameters, out slots and callback contexts remain pointers.
Address-based hashes and allocator range bookkeeping (`heap_maps.c`) can use
`uintptr_t` intentionally; normal field/index/header access must not pass through
integer address arithmetic. Counts, lengths, capacities, byte sizes and offsets
are integer quantities, not references.

JS handles are externally assigned IDs, not native payload pointers. JS tagged
slot tags/aux words and wasm linear-memory offsets remain integer transport.
The broad existing `AbiTy::Ptr` classification includes `TyKind::Js`, so changing
that class mechanically without distinguishing JS handle storage is unsafe.

The host migration surface includes `dream-host-abi/src/lib.rs` and all capability
exports, especially the core-owned allocation/completion callback table in
`dream-host-core/src/exports/abi.rs`: its current `usize` results/arguments carry
actual guest addresses. Other capabilities must share that table rather than
duplicate it. Generated C/C++ marshalling, static immortal literals, globals,
function/itable initializers, debug DWARF views, sidecar schema and cache stamps
also require coordinated review. Rebuilt bitcode's `RuntimeSigs` is authoritative;
automatic pointer/integer coercion must not hide stale native ABI signatures.

## Baseline protocol

Source revision: `dbae3e7d260b50d75387f4e69098c2c479cac129` (merged Phase 3.5).
Compiler: existing `target/debug/dream`, built October 3, 2026; no Cargo rebuild
was performed for this baseline. Guest optimization is release `-O3`, independent
of the compiler executable's own build profile. The older October 1 release
compiler was deliberately not used.

Host: Apple M3, arm64 macOS, Darwin 25.6.0. Generated target:
`arm64-apple-macosx11.0.0`, pointer width/alignment 8 bytes. LLVM 22.1.8,
revision `ca7933e47d3a3451d81e72ac174dcb5aa28b59d1`. Whole-program optimization:
`internalize,default<O3>`; native link includes `-Wl,-dead_strip`. Runtime and
language sources were captured before native reference changes.

The language suite is unchanged `tests/bench/microbenches.dream`: compile once,
one discarded process start, then three starts with `DREAM_BENCH_PASSES=3` each.
The suite also performs its own unmeasured warmup. Each process's three samples
are collapsed to a median; reported value is the median of the three process
medians and spread is `(max - min) / median`. Concurrent development activity
was present; this is a noisy baseline, not evidence of a speedup or regression.
No allocation/retain/release event counters were collected; timings do not
substitute for those counts.

For post-migration replay, the preserved old binary resolves `dream_host_bind`
from the October 2 release `libdream_host_core.dylib`, copied into
`target/audit-p3-pointer-baseline/host-abi-v1/`. Only the **before** process receives
that directory through `DYLD_LIBRARY_PATH`; the after process uses the current
adjacent development library. `nm -u` confirms the old benchmark binary's only
Dream host-library dependency is `dream_host_bind`, called at startup: timed
kernels, time queries, environment lookup and printing do not call core-library
helpers. The older release host artifact is therefore a binding/setup distinction,
not a timed-kernel implementation difference. This isolated benchmark artifact is
not shipped and does not introduce a dual ABI into the compiler or runtime.

Reproduce the language artifacts with an existing built compiler:

```bash
target/debug/dream --release -o "$PWD/target/audit-p3-pointer-baseline/microbench.ll" tests/bench/microbenches.dream
~/.dream/toolchains/llvm-22.1.8/bin/llc -O3 -filetype=asm target/audit-p3-pointer-baseline/microbench.opt.ll -o target/audit-p3-pointer-baseline/microbench.s
DREAM_BENCH_PASSES=1 target/audit-p3-pointer-baseline/microbench.bin
# Repeat three times, retaining each stdout separately:
DREAM_BENCH_PASSES=3 target/audit-p3-pointer-baseline/microbench.bin
```

Use `.ll` for `-o`, not `.bin`: the baseline CLI aliases its generated IR and
binary output when given `.bin`, then removes the binary during IR cleanup.
The frontend reference fixture's `.ll` was copied while compilation was running,
before that cleanup. `reference-paths.raw.ll` preserves native class aliasing,
string and array access; `reference-paths.opt.ll` and `.s` preserve optimized
whole-program output. The initial library-only experiment was not used: it listed
public exports in metadata but pruned their bodies, making it unrepresentative.

## Pre-migration measurements

Language executable: **559,536 bytes**. Whole-program optimized IR: **3,731,607
bytes**. Assembly text: **2,104,441 bytes**. These are raw artifact sizes, not
distribution sizes and not a release host-library baseline.

The optimized benchmark module contains 1,386 `inttoptr` and 848 `ptrtoint`
occurrences. The smaller frontend reference fixture contains 41 and 16
respectively; its optimized module contains 31 and 34. Whole-module totals include
runtime code and legitimate hashing/transport conversions, so they are not a
count of avoidable conversions. In the frontend fixture, ordinary `read_alias`,
`string_count` and `array_read` take `i64` reference parameters. `read_alias`
contains three direct `inttoptr` operations for its ordinary class accesses;
the string/array fixtures pass integer handles into runtime access helpers.
Static string literals are `ptrtoint` constant
expressions. These are structural examples for the migration's regression tests,
separate from intentional registry/address hashing boundaries.

Representative median nanoseconds per reported operation, with process spread:

- `linked_walk`: 2,518.00 ns, 30.5%.
- `arr_add`: 108.29 ns, 5.3%; `vec_add`: 62.92 ns, 27.5%.
- `matmul_64`: 124,330.00 ns, 10.5%.
- `char_scan`: 39.90 ns, 9.3%; `byte_scan`: 39.16 ns, 12.4%.
- `string_builder`: 26.02 ns, 42.3%.
- `map_get_set`: 8.50 ns, 29.2%.
- `binary_trees`: 110,080.00 ns, 21.0%.
- `arc_locals`: 20.18 ns, 17.5%.

All suite families were run, not only these representative rows. Raw samples are
`native.rep{1,2,3}.raw`; zero/sub-timer measurements must not be interpreted as
zero-cost operations. Re-measure after migration with the same executable flags,
source and warmup protocol, ideally with other builds stopped. Investigate material
regressions instead of requiring an arbitrary improvement percentage.

The historical C hotpath used a separate minimum-based harness, now retired. Current runtime attribution uses `scripts/bench.py --arms current --counters`:
`-O3 -flto -march=native`, minimum of five runs for each kernel. The pinned LLVM
22 clang's LTO objects cannot be read by this machine's Apple LLVM 21 linker
(`Unknown attribute kind (105)`), so this separate baseline used Apple clang
21.0.0 (`clang-2100.1.1.101`), with the macOS SDK supplied via `SDKROOT`. Use the
same C compiler after migration; do not compare it as if it were LLVM 22 data.
The preserved `runtime-c/` snapshot allows that exact source to be rebuilt.

Captured C hotpath: allocator 17.91 ns/op, character scan 0.50 ns/op,
reserved builder append 9.41 ns/op, growing builder append 44.35 ns/op.
Substring/concat report 0.00 at the script's precision; those values cannot
support performance claims. No sanitizer or stress execution was part of this
baseline. No Linux/Windows execution was available; cross-emission is not native
execution evidence for either platform.

## Paired comparison after implementation

The stable implementation measured here is revision
`0469ddb4f3fe90191ac2c122e57b598e976b75ad`. The workspace build, strict Clippy and
tests had completed before measurement; no Cargo or corpus probes ran during the
timed section. Compilation again used the existing debug compiler executable with
guest `-O3`, LLVM 22.1.8, the same target/deployment version and dead-strip policy.
There are no experimental optimization attributes or A/B runtime switches.

`target/audit-p3-pointer-baseline/compare.sh` compiled the after executable once,
warmed both variants, then alternated before/after starts for three repetitions
with three measured passes each. Before uses the isolated old binding library
described above; after uses the current host ABI. Both variants completed every
suite family. Raw paired samples, build flags, assembly, live checks and optimizer
remarks remain under `target/audit-p3-pointer-after/`. This directory and the
old-ABI library are local measurement artifacts, not distribution inputs.

The executable grew from 559,536 to **575,840 bytes** (+16,304 bytes, **2.9%**).
Optimized whole-program IR grew from 3,731,607 to **3,902,779 bytes** (+4.6%);
assembly text grew from 2,104,441 to **2,184,673 bytes** (+3.8%). Text-file sizes
are not instruction counts. The optimized module's integer/pointer conversions
dropped from 1,386 `inttoptr` / 848 `ptrtoint` to **9 / 29**. Remaining conversions
include explicit result-word/address transport and are not a claim of zero address
conversions. Removing IR casts alone does not demonstrate faster machine code.

Paired process-median nanoseconds per operation, with before/after process spreads:

- `linked_walk`: 1,301.00 → 1,306.75 ns (17.1% / 8.6% spread); neutral.
- `arr_add`: 49.16 → 49.10 ns (10.4% / 12.5%); neutral.
- `vec_add`: 23.49 → 23.74 ns (12.8% / 8.3%); neutral.
- `matmul_64`: 52,370.00 → 52,282.50 ns (14.3% / 11.0%); neutral.
- `char_scan`: 16.93 → 16.91 ns (11.9% / 11.8%); neutral.
- `byte_scan`: 18.03 → 17.95 ns (7.3% / 6.2%); neutral.
- `string_builder`: 12.73 → 12.28 ns (16.6% / 4.6%); within observed spread.
- `binary_trees`: 43,680.00 → 41,790.00 ns (15.0% / 8.4%); within observed spread.
- `arc_locals`: 7.29 → 7.59 ns (14.6% / 5.8%); within observed spread.
- `map_get_set`: 4.23 → 4.91 ns (12.4% / 33.7%); candidate regression, not resolved
  by this noisy run.

Other notable candidates are `list_push` 2.36 → 0.84 ns and `string_eq` 2.96 →
2.24 ns, larger improvements than their measured process spreads, and
`map_clear_reuse` 1.82 → 2.11 ns with 105% after-spread. The map rows deserve a
longer isolated rerun before claiming a performance regression or benefit. The
paired run is substantially faster than the original busy-machine baseline on
both variants, which demonstrates why the original absolute timings were not used
as the comparison denominator. No universal speedup is claimed.

The C hotpath executable was rebuilt with the same Apple clang 21 compiler and
flags, and the preserved before executable was replayed beside it. Both report
allocator 4.54 ns/op and character scan 0.10 ns/op; reserved builder append is
3.30 → 3.31 ns/op and growing append 15.45 → 16.25 ns/op. These use the existing
minimum-of-five protocol rather than process medians; no confidence interval is
available. Zero-precision substring/concat samples remain uninterpretable.

Separate, untimed `DREAM_DEBUG_LEAKS=1`, one-pass suite executions both reported
**live=0**, **total_allocations=4,885,241**, and no leaked types. These include the
suite's own warmup. Retain/release event counters are unavailable; no instrumentation
was introduced, so allocation equality is not an ARC-call-count measurement.

### Optimized IR and assembly observations

Generated reference paths now preserve pointer signatures and fields; structural
regressions cover aliases, inline managed fields, pointer null and direct niche
`None`, closure environments versus integer selectors, async frames and the wasm
offset boundary. The benchmark's kernels are largely inlined into `run_suite`
already; the assembly still retains the combined function and native arithmetic
loops. Reference casts often had no dedicated machine instruction before the
migration, so their removal cannot be equated with instruction savings.

For a repeatable remaining-opportunity inspection, the same additional
`default<O3>` pipeline was run over each *already optimized* module with saved
optimization remarks. These are not first-pass optimization records and cannot
attribute the original pipeline's wins. Remaining GVN `LoadClobbered` remarks
increase 15,018 → 18,493, while LICM invalidated-address remarks decrease
3,073 → 3,018; additional GVN load-PRE remarks are 7 → 40 and LICM hoists 11 → 19.
SLP stores-vectorized remarks are 28 → 61, but array/vector timing is neutral.
No-definition inline remarks remain 391 → 454 for external declarations;
whole-program runtime visibility is retained on both variants. This is mixed
optimization evidence, not justification for blanket alias facts. Representation,
synthetic-local classification and the coordinated runtime ABI changed together;
the measurements do not isolate one of those components as the cause of a gain.

## Remaining execution evidence

Any optional LLVM fact needs a semantic proof producer and negative regression.
Pointer representation alone proves neither exclusivity nor `noalias`, and this
record supplies no justification for blanket TBAA, alias scopes, `invariant.load`,
`nonnull` or `inbounds`. Runtime visibility already permits LLVM's own inference.
Cross-target IR emission does not prove Linux/Windows execution. The parent audit
tracker records the final corpus, ABI/layout, determinism and platform checks;
Windows completion must be confirmed there before declaring Phase 3.7's entire
execution matrix complete. The map timing candidates above remain suitable for
follow-up measurement rather than an invented speedup claim.
