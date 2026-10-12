# Benchmark measurement protocol

## Measurement protocol (read before comparing numbers)

Use `scripts/bench.py` for regression decisions. Timing runs compile without runtime
counters. `--counters` builds a separate instrumented runtime for attribution and cannot create
or pass a performance reference.

`--target wasm --arms current` measures the same Dream workloads through Node. A wasm
reference preserves the module, selective JS host and launcher together, and records Node
and Binaryen identities. Native and wasm references are incompatible; Node process RSS
includes the engine and JIT. Rebuild references after changing the fixture or harness.
`--benchmark <path>` selects a preserved fixture snapshot; each invocation reads it once for
all arms and fingerprints those exact bytes, so editing the live fixture cannot mix arm inputs.
`--diagnostics` also saves first-pass remarks and optimized IR, plus native assembly or WAT;
C# diagnostics save JIT disassembly for n-body, Fibonacci and quicksort. .NET SDK, host and
installed runtime binaries are fingerprinted alongside the benchmark assembly.
`build.json` records compile duration and artifact size. Compile duration includes diagnostic
emission when requested and must be compared with matching flags.

`gate.json` distinguishes regression acceptance (`decision`) from a demonstrated speedup
(`improvement`: the upper bound of the paired ratio's 95% interval is below 1.0). Passing the
10% regression gate alone does not establish a win.

The fixture includes `closure_dispatch`, `async_poll`, `scheduler_queue`, `set_probe` and
`set_clear_reuse` (42 workloads in total). Runtime attribution
adds non-null retain/release helper events, accepted ready-queue enqueues and queue-node heap
allocations to the existing
weak/region counters. Immortal no-op retain/releases are included; these are helper events,
not an instruction count. Attribution runs remain separate from timing runs.

```sh
scripts/bench.py --target wasm --arms current --runner-id controlled-runner \
  --save-baseline target/wasm-reference.json
scripts/bench.py --target wasm --arms current --runner-id controlled-runner \
  --gate target/wasm-reference.json --rounds 20
scripts/bench.py --target wasm --arms current --counters --rounds 1 --passes 1
```

Use a quiet, controlled runner with a stable `--runner-id`. Arms rotate across process starts;
two discarded warmups precede twenty measured rounds with ten passes each. A round contributes its
median pass time. The gate bootstraps paired current/reference round ratios and reports their
95% confidence interval: an upper bound at or below 1.10 passes, a lower bound above 1.10 fails,
and an overlapping interval is inconclusive (exit 2). Inconclusive measurements can be repeated
with `--rounds 40` after resolving background load. Neither minima nor noisy hosted-runner results establish acceptance.

Recompile historical Dream against the current fixtures before recording a reference:

```sh
scripts/bench.py --arms baseline current --runner-id controlled-runner \
  --save-baseline target/perf-reference.json
scripts/bench.py --arms current --runner-id controlled-runner \
  --gate target/perf-reference.json
```

The versioned reference contains complete samples, intervals, fixture/input fingerprints,
hardware identity, compiler hashes, tool identities and per-process peak RSS. It archives the
reference executable and replays that executable in paired control rounds when gating. Keep its
linked host libraries available. References from another runner or fixture version, corrupted
executables, invalid timing rows and incomplete measurements are rejected. Old median-only
reference files must be regenerated. Peak current RSS must remain within the configured regression budget of the live control.

Dream and C# comparisons are reported separately from Dream regression decisions. Both arms use
`DREAM_BENCH_SEED` for matched runtime-generated inputs. The comparator derives fractional
ns/op from `ns_total/iters`; zero-time workloads invalidate a reference or gate. Hosted CI publishes
raw measurements for inspection; hard gates require a controlled runner.

## API fairness notes

`regex_find` measures public result APIs: Dream returns matched strings, while C# reads
`Match.Length` without materializing those strings. Native Dream uses PCRE2 JIT.
`binary_trees` includes immediate ARC reclamation in Dream but permits deferred C# GC.
`binary_trees_reclaim` is a separate measurement with C# collection explicitly inside
the timer and a clean heap beforehand. Forced full collections add synthetic overhead;
this row does not model normal GC scheduling or establish a general ARC/GC ranking.
The reclamation row runs last so its forced GC cannot perturb other rows in that pass.

The span rows use stack views on both sides. Owned substring is a separate contract: Dream
retains a slice and C# copies. Array and vector addition consume checksums of every output
outside the timer. Every row emits exact elapsed nanoseconds, iteration counts and a result
checksum; mismatched outputs or counts invalidate the comparison. Confidence intervals use
process medians, never independent resampling of correlated passes.

There is one entry point:

```sh
scripts/bench.py                       # native Dream, 20 rounds × 10 passes
scripts/bench.py --arms current csharp # explicit Dream/C# micro comparison
scripts/bench.py --suite macro --arms current csharp # application comparison
scripts/bench.py --arms current        # Dream inventory without .NET
scripts/bench.py build --help          # cold, edited and cached compiler builds
scripts/bench.py generators --help     # isolated source-generator cache measurements
```

The old minimum-based C hotpath harness is retired as a measurement entry point. Language
workloads exercise the allocator, weak references, strings and queues with matched inputs.
Counter runs attribute mechanisms but cannot establish latency wins. Different lifecycle/engine
contracts are explicitly marked and excluded from suite-wide C# victory claims.


Preserve machine-specific results as immutable JSON with raw samples and runner identities
under ignored `tests/bench/out/` directories. Do not commit benchmark results or historical timing tables. Re-run comparisons on
the deployment hardware; results from one runner are not acceptance for another.
