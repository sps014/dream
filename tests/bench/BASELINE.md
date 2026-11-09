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
`set_clear_reuse` (41 workloads in total). Runtime attribution
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

## Rust and Go comparisons

With Rust and Go available on `PATH`, first build the compiler with
`cargo build --release -p dream`.

`scripts/bench.py compare --suite micro --rounds 3` builds the same micro workloads in
Dream, Rust, and Go. Use `--against rust` or `--against go` for a two-language comparison.
`scripts/bench.py compare --suite macro --rounds 3` runs all seven application workloads
against both Rust and Go. The manual **Language benchmark report** workflow runs both suites.

Dream uses `--release`, Rust uses Cargo's release profile at optimization level 3, and Go
uses its default optimized build. Each process warms up once. Process order rotates between
rounds, and the report compares median nanoseconds per operation. Missing workloads, changed
checksums, mismatched iteration counts, and nonpositive times invalidate the comparison.
JSON preserves each sample, build command, source hashes, and tool versions; raw process
output is saved beside it. Reports default to ignored `target/language-report/` storage.

These are native library comparisons: Rust uses `Rc`/`Weak`, standard collections,
`serde_json`, `regex`, directly awaited futures, `join_all`, and Rayon parallel jobs. Tokio
drives cooperative yields without adding spawned tasks to direct-await rows. Dream uses ARC, UTF-16 strings, PCRE2, and its
cooperative scheduler; Go uses GC, UTF-8 strings, and goroutines. Inputs are ASCII, but string
encoding, ownership, and scheduling costs differ. Owned substring rows retain the source
buffer in all three ports, with varying bounds and observed contents. The explicit
`binary_trees_reclaim` row completes reclamation in all ports, using `runtime.GC` in Go.
Fanout races wait for remaining work before stopping the timer. Array-add rows feed each
result into the next iteration so dead stores cannot remove repeated additions.
There are no per-operation optimization barriers. All ports expose results through their sinks.
Rust's byte scan hides the buffer once per iteration; otherwise the constant reduction
folds into `iters * sum` and the row measures nothing. Rust's direct-await helpers stay
out of line, matching Dream's separate async functions.
Rows below 10 microseconds receive no winner verdict. Go's goroutine-based `async_poll`
and `scheduler_queue` are marked not equivalent to a direct future await. Reports distinguish workload, runtime,
library, and representation comparisons; they must not be combined into a language speed score.
Ratios are descriptive measurements on one machine, not regression acceptance or universal
language rankings. Keep machine-specific reports out of commits.


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

The macro suite reports `graph_paths_class` and `graph_paths_value` separately. Both store the graph as value structs, matching the Go port: a `List<Vertex>` of inline vertices, each holding a contiguous `List<Edge>` of inline edges. Only the queue entries differ: they are classes in both class arms and inline values in both value arms. `graph_paths_heap` runs the same queries over a fully heap-allocated graph: class vertices, class edges and class queue entries in Dream, and `*heapVertex`, `*edge` and `*visit` in Go. A heap graph spends most of its time following one pointer per edge, and that cost dominates once the graph outgrows the CPU's L2 cache. A Dream object carries a 16-byte header where a small Go object carries none, so the heap row is the one most sensitive to cache size: an Apple M3's large L2 hides it, while the smaller L2 on x86 Linux runners does not. Graph construction is outside timing. Each query creates its own distance buffer and queue. Graph counters are sampled after setup, immediately before the query loop.

Use `--suite macro --filter graph_paths_class graph_paths_value` to execute only the graph workloads in both arms. The same selection works with `bench.py profile --suite macro --filter graph_paths_class`; the fixture receives the comma-separated `DREAM_BENCH_ONLY` selection. Timing, counters, sampling and sanitizers must run separately and sequentially. Full-suite references and gates still reject filtered runs.
