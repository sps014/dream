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
