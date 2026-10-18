# Benchmark measurement protocol

## CI regression gate

The **Benchmark regression gate** workflow runs on pull requests, pushes to `main`, and manual
dispatches. It builds Dream and its host libraries from the checked-out revision and from the
base revision, then runs `bench.py --arms baseline current --fail-on-regression` for the micro
and macro suites on the same runner. Arms alternate order between rounds. A row whose paired
current/base interval lies entirely above the 10% budget fails the job.

The job summary includes timings, confidence intervals, ratios, and peak RSS. The
`dream-bench-<revision>` artifact keeps the comparison reports. Hosted runners are noisy, so
the controlled reference gate below remains the acceptance measurement.

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
`--diagnostics` also saves first-pass remarks and optimized IR, plus native assembly or WAT.
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

Both arms use `DREAM_BENCH_SEED` for matched runtime-generated inputs. The comparator derives fractional
ns/op from `ns_total/iters`; zero-time workloads invalidate a reference or gate. Hosted CI publishes
raw measurements for inspection; hard gates require a controlled runner.

## Result validation

Every row emits exact elapsed nanoseconds, iteration counts and a result checksum; mismatched
outputs or counts between arms invalidate the comparison. Confidence intervals use process
medians, never independent resampling of correlated passes. `binary_trees_reclaim` runs last so
its reclamation work cannot perturb other rows in that pass.

There is one entry point:

```sh
scripts/bench.py                       # native Dream, 20 rounds × 10 passes
scripts/bench.py --arms baseline current # paired comparison against a reference build
scripts/bench.py --suite macro         # application workloads
scripts/bench.py build --help          # cold, edited and cached compiler builds
scripts/bench.py generators --help     # isolated source-generator cache measurements
```

The old minimum-based C hotpath harness is retired as a measurement entry point. Language
workloads exercise the allocator, weak references, strings and queues with matched inputs.
Counter runs attribute mechanisms but cannot establish latency wins.


Preserve machine-specific results as immutable JSON with raw samples and runner identities
under ignored `tests/bench/out/` directories. Do not commit benchmark results or historical timing tables. Re-run comparisons on
the deployment hardware; results from one runner are not acceptance for another.

## Parallel task attribution

`task_parallel` submits sixteen CPU jobs per batch through `Task.scope` and
`scope.spawn_parallel` on the runtime's shared worker threads. It times submission,
completion and reduction, including the per-job session ownership. Each job executes the
same integer kernel and the final checksum is validated.

The `workers` suite also measures sequential `task_spawn` calls and tiny structured
bursts. `task_scope_16` and `task_scope_64` report nanoseconds per whole batch of sixteen or
sixty-four jobs, respectively; the other worker rows report nanoseconds per dispatch/session.
Keep those units separate when comparing submission overhead with the CPU-heavy macro row.

## Graph queue attribution

The macro suite reports `graph_paths_class` and `graph_paths_value` separately. Both keep class-based graph vertices and edges; queue entries are classes in both class arms and inline values in both value arms. Graph construction is outside timing. Each query creates its own distance buffer and queue. Graph counters are sampled after setup, immediately before the query loop.

Use `--suite macro --filter graph_paths_class graph_paths_value` to execute only the graph workloads in both arms. The same selection works with `bench.py profile --suite macro --filter graph_paths_class`; the fixture receives the comma-separated `DREAM_BENCH_ONLY` selection. Timing, counters, sampling and sanitizers must run separately and sequentially. Full-suite references and gates still reject filtered runs.
