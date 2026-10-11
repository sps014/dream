# Runtime stabilization and performance campaign (Oct 9 2026)

Runner `local-m3` (Apple M3, AC power, no background builds while measuring). References:
`target/perf/native-head-ref.json` and `target/perf/wasm-head-ref.json`, taken on `fc499d07`
with `--rounds 10 --passes 5 --warmup 1`. Every perf change in this campaign is gated against
them with `scripts/bench.py --gate`.

## The 7x `fib_rec` swing is codegen, not noise

`fib_rec` read ~40-52k ns/op in the `hotloops` campaign (`09af6ead`) and ~5-7k ns/op from the
`nbody-span` campaign on. The fixture did not change. Current LLVM output proves `fib` pure,
inlines one level of the recursion and merges the duplicated `fib(n-3)` call:

```
fib(n) = [fib(n-4) + fib(n-3)] + [fib(n-3) + fib(n-2)]   ; fib(n-3) computed once
```

The call tree now grows as the real root of x^4 = x^2 + x + 1 (~1.465^n) instead of phi^n
(~1.618^n); at n = 20 that is (1.618/1.465)^20 ≈ 7.3x fewer calls, matching the swing. C#
executes every call, so `fib_rec` is marked `different-contract` in `scripts/benchmarks/stats.py`.
The C# arm's own 34k-48k spread between trials is tiered-JIT and thermal drift; it is why
comparisons rotate arms ABBA-style inside one campaign and never compare across campaigns.

## Native vs wasm (HEAD, ns/op, sorted by gap)

| bench | native | wasm | wasm/native |
|---|---|---|---|
| vec_add | 37.37 | 191 | 5.12 |
| map_get_span | 5.34 | 16.30 | 3.05 |
| split_span | 41.78 | 120 | 2.88 |
| alloc_churn | 12.03 | 32.89 | 2.73 |
| binary_trees_alloc | 27,955 | 73,934 | 2.64 |
| wordcount | 17.22 | 44.04 | 2.56 |
| arc_locals | 11.72 | 29.22 | 2.49 |
| iface_dispatch | 3.04 | 7.57 | 2.49 |
| substring | 1.66 | 4.10 | 2.47 |
| string_builder | 6.93 | 16.47 | 2.38 |
| sieve | 2,135 | 4,962 | 2.32 |
| list_push | 0.67 | 1.46 | 2.20 |
| json_deserialize | 889 | 1,822 | 2.05 |
| json_serialize | 246 | 488 | 1.98 |
| map_get_set | 3.39 | 6.19 | 1.82 |
| set_probe | 11.98 | 21.06 | 1.76 |
| binary_trees | 47,285 | 40,547 | 0.86 |
| char_scan | 24.27 | 20.32 | 0.84 |
| closure_dispatch | 3.23 | 1.49 | 0.46 |

Rows within ±25% are omitted. Peak RSS: native 85 MB (the old ~1 GB gap is closed); wasm under
Node 757 MB, because linear memory only grows.

Native loses to wasm on `binary_trees` and `closure_dispatch`, which points at native-only
costs (allocator release path and indirect-call lowering), not at wasm being fast.

## Macro workloads (`scripts/bench.py --suite macro`)

Application-shaped work in `tests/bench/macro/`, with C# twins whose integer checksums must
match exactly. First single-process reading (native, ns/op):

| bench | Dream | C# | Dream/C# |
|---|---|---|---|
| json_service | 8,768 | 44,886 | 0.20 |
| log_processor | 1,753,700 | 1,067,583 | 1.64 |
| graph_paths | 2,057,640 | 808,327 | 2.55 |
| async_fanout | 83,565 | 123,124 | 0.68 |
| task_parallel | 2,202,440 | 1,135,081 | 1.94 |

Unlike the micro suite, three of five application workloads lose to C#. `graph_paths`
(class nodes, `PriorityQueue` of small objects, ARC traffic), `task_parallel` (worker spawn and
cross-thread frees) and `log_processor` (split, concat, `Map<string, int>`) set the priorities
for the allocator, remote-free and hashing work below.

## Phase 2: wasm allocator and runtime inlining (2026-10-10)

The wasm guest now inlines the thread-local freelist `malloc`/`recycle` fast paths, and only the
libc-defining units (`WASM32_LIBC_UNITS`) build with `-fno-builtin`, so runtime header helpers
inline into Dream code. Cold paths that inlining would drag into hot loops are `cold, noinline`
(`dream_array_realloc*`, `dream_retain_slow`, the weak-observed count paths). String equality on
wasm compares 8-byte words directly: an inlined `memcmp(...) == 0` reached codegen behind a
`freeze` and expanded into the ordering form built from byte swaps wasm does not have.

Paired wasm run against `fc499d07` (8 rounds × 5 passes, loaded machine):

| bench | cur/base | | bench | cur/base |
|---|---|---|---|---|
| vec_add | 0.18 | | json_deserialize | 0.70 |
| binary_trees_alloc | 0.43 | | split_span | 0.77 |
| alloc_churn | 0.56 | | weak_tree | 0.77 |
| string_eq | 0.56 | | json_serialize | 0.80 |
| arc_locals | 0.57 | | scheduler_queue | 0.83 |
| set_probe | 0.68 | | regex_find | 0.89 |
| substring, sum_options | 0.68–0.69 | | **binary_trees(_reclaim)** | **1.41–1.45** |
| | | | **closure_dispatch** | **1.29** |

Other rows are inconclusive within ±5%. The three regressions are V8 speculative-inlining
effects, not codegen: the hot loops and the closure body are byte-identical to the baseline's,
and under `node --no-wasm-inlining` all three tie (1.00–1.05). V8 inlines more into the
baseline's smaller functions; raising `--wasm-inlining-budget` or `--wasm-inlining-max-size`
does not change it. Accepted for the wins above; revisit if function size can be cut without
losing the runtime inlining.

Native rows `substring`, `map_get_span` and `json_serialize` that drifted in Phase 1 have
identical machine code across builds (code layout). Follow-up: `leak_diagnostics` can flake while
the runtime cache rebuilds concurrently.

## Phase 2: wasm calling convention (2026-10-11)

Measured before building; only the feature-flag change shipped.

- **Multivalue span returns: no.** After inlining, only `format_g` still returns a value type
  through a caller buffer (10 wasm call sites, 2 native) in the microbenches, and none in the
  macro suite. LLVM's multivalue lowering also needs `-target-abi=experimental-mv`, which changes
  libcall signatures (`__multi3` and other i128 helpers) away from the prebuilt compiler-rt.
- **SSA `Vector<T>`: no.** SROA already keeps `vec_add` in `<4 x float>` registers; the wasm hot
  loop is unaligned `v128` load/add/store unrolled ×2 with no allocas or bounds checks.
- **Interface guard: working.** On wasm the four-implementor `iface_dispatch` loop switches on the
  tag with all four `apply` bodies inlined; the itable call is only the fallback. The remaining
  2.5× gap is the for-each retain of each element plus an out-of-line `dream_release_object` of
  the previous one, which is ARC elision work (borrowed iteration), not ABI.
- **Binaryen features: shipped.** `wasm-opt` was forced to enable exception handling, tail calls,
  relaxed SIMD, extended-const and multi-memory, none of which the module used, and wrote them
  into the output's `target_features` section (relaxed SIMD alone rules out Safari). It now
  reads features from wasm-ld's section. Microbench module 787,247 → 787,083 B, same compile
  time; C++ exception packages still detect EH (`wasm_interop` passes at all levels).

## Phase 3: Task fan-out (2026-10-11, Windows x64)

New runner, so these are fresh baselines and not comparable with the M3 rows above. Peak RSS
before the change: native micro 88 MB, native macro 21 MB (C# 58 MB), wasm micro under Node 490 MB.

`task_parallel` was 10.6× C#, and slower than doing the 16 chunks serially (736 vs ~88 µs/op).
Two causes:

- Every `Task` session started and joined its own OS thread: 16 empty tasks cost 759 µs. A
  finished worker now parks its thread (up to 64) for the next `workerSpawn`; `workerTerminate`
  waits for the session to finish instead of joining the thread. Empty fan-out: 759 → 47 µs.
- `Task.map` awaited lazily-started per-item spawn futures in order, so items ran one at a time.
  It now posts every item before the first await (`TaskGroup` owns the sessions).

| macro (3 rounds) | before cur/C# | after cur/C# |
|---|---|---|
| task_parallel | 10.57 | 1.76 |

Macro peak RSS 21.1 → 8.3 MB. `TaskGroup` teardown waited for each worker in turn; a new
`workerShutdown` host stops every member before `workerTerminate` waits on any (a no-op in
JS, whose terminate never blocks). Empty 16-way fan-out 78 → 69 µs; `task_parallel` 1.22× C#
(5 rounds). The remainder is thread wake-up latency. A spin-before-park experiment was not
measured and is not shipped.

## Phase 3: regex JIT and wasm large blocks (2026-10-11, Windows x64)

- **PCRE2 JIT was never on.** `regex.c` gated `pcre2_jit_compile` on `SUPPORT_JIT`, which only
  the vendor `config.h` defines and `regex.c` never includes, so native regexes ran in the
  interpreter. Now gated on `DREAM_NATIVE`. `regex_test` also matches directly with a reused
  per-thread match block instead of building and dropping a capture array per call.
  Native `regex_find` 0.35×; `log_processor` 2.13 → 1.32× C#.
- **Wasm linear memory grew on every pass with no leak.** Private allocations past the 64 KB
  classes were bump-allocated from the slab but freed onto the large list, so they were never
  reused, and each large request also abandoned the rest of the 2 MB slab. They now come from
  the large list first (or a claim of exactly their size), and a refill returns the old slab's
  tail to that list. Linear memory after 1/3/6 passes: 233/449/774 MB → 68.7 MB flat. Peak RSS
  under Node 490 → 143 MB. Wasm `string_builder` 0.36×, `list_push` 0.62×, `set_probe` 0.61×;
  other rows within run-to-run drift.

| macro (3 rounds) | Phase 3 start | now |
|---|---|---|
| task_parallel | 10.57 | 1.22 |
| log_processor | 2.15 | 1.28 |
| async_fanout | 1.13 | 0.90 (inconclusive) |
| graph_paths | 0.66 | 0.67 |
| json_service | 0.13 | 0.12 |

Measured and not pursued:

- **Smaller object header:** memory is no longer the constraint. Native micro peaks at 88 MB and
  stays flat across passes, the macro suite at 8 MB against C#'s 57 MB, and wasm linear memory
  at 69 MB. A 16 → 8 byte header would touch the layout constant shared by codegen and every
  runtime unit, for a few percent of footprint on allocation-heavy rows.
- **Lock-free cross-thread frees:** no workload frees across threads in a measurable amount.
  `task_parallel`'s remaining cost is worker wake-up and teardown, and the other macro losses
  run on one thread.

## Borrowed for-each and borrowed-param ARC elision (2026-10-11, Windows x64)

`rc-held-by-owner` (`passes/rc/held.rs`) now accepts two more snapshots. A snapshot moved into
another local keeps its class, so the for-each binding no longer retains each element and
releases it again. A copy of a borrowed parameter is held by the caller for the whole call,
unless some copy of it is returned or handed to a sink, where the copy's retain is that +1.
`iface_dispatch`'s loop body and `sum_nodes` over a `borrow List<Node>` now have no atomics,
and LLVM vectorizes the latter.

Current against HEAD compiled by the same runner (micro 8 rounds native / 6 wasm, macro 8):

| bench | native cur/base | wasm cur/base |
|---|---|---|
| iface_dispatch | 0.30 | 0.37 |
| parse_ints | 0.86 | 0.74 |
| string_eq | 0.91 | 1.00 |
| map_get_span | 0.93 | 1.00 |
| binary_trees | 1.11 | 1.00 |

Only `bench_iface_dispatch` and `bench_parse_ints` differ in the optimized microbench IR, so
`string_eq` and `map_get_span` were not changed by the pass. Native `binary_trees` repeats at
1.11, but `make_tree`, `destroy_TreeNode` and the release functions are identical in the
assembly too; the two functions that shrank moved these hot functions to new addresses. On
wasm the row is flat. In the macro suite
`make_graph` drops from 16 RC operations to 2 and `shortest_sum` from 115 to 99; `graph_paths`
0.98, the other rows within drift. Peak RSS unchanged.
