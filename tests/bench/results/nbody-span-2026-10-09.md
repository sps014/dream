# N-body and borrowed substring spans — 2026-10-09

Native release builds on macOS arm64, Apple M3, LLVM 22.1.8 and .NET SDK 10.0.102.
The Dream and C# fixtures are unchanged. The baseline uses the compiler preserved before
this task; current uses the completed changes. Compiler, fixture and executable hashes,
all 42 rows, process medians and both trials are in the [JSON record](nbody-span-2026-10-09.json).

## Changes

- `string.span(start, end)` takes a short unsigned range check for valid bounds and retains
  its existing clamping semantics for invalid bounds. `@inline` keeps the larger method
  within the inlining budget so borrowed views remain borrowed. Late borrowed-field
  forwarding now runs SCCP/CFG cleanup/DCE, invalidates CFG analysis and reruns string
  payload hoisting; a private borrowed view's cleanup no longer blocks that hoist.
- The late `loop-fields` MIR pass reuses scalar class fields unchanged within a two-block
  loop, including across stores to different fields through aliases. It peels the entry
  test and complete first iteration so zero-trip behavior and the initial order of reads
  and traps are preserved. Shared objects, same-slot writes, raw/global stores, ownership
  effects, async functions and unknown calls reject the proof. Numeric math calls reuse
  the existing pure-import registry. Floating-point operation ordering is preserved.
  N-body positions and mass can stay in registers across pair updates, with fresh values
  on each new loop entry.

## Measurements

Each arm discards one full process before measurement; the fixture also warms its suite.
Seed is 1, with five measured passes per process. Confidence intervals use the existing
4,000-resample process-median bootstrap. Ratios are medians of paired process ratios;
they need not equal quotients of the per-arm medians shown below. Builds and correctness
probes did not overlap timing. Background desktop activity remained uncontrolled.

The initial 10-round trial used three cyclic arm orders:

- `nbody`: Dream 88.812 → 74.188 ns/op; paired current/baseline **0.822** (95% CI 0.749–0.926). C# median 72.666 ns/op; paired current/C# **0.994** (0.898–1.267), inconclusive.
- `substring_span`: Dream 1.238 → 0.933 ns/op; paired current/baseline **0.754** (95% CI 0.651–0.838). C# median 1.067 ns/op; paired current/C# **0.810** (0.729–0.962), win.

The follow-up was specified after seeing broad timing uncertainty in the first trial.
It uses 12 rounds with all six arm permutations twice, balancing both position and pair
order. It was run after the complete corpus. The initial trial remains in the record;
the trials are not pooled or selectively discarded.

- `nbody`: Dream 134.919 → 100.719 ns/op; paired current/baseline **0.759** (95% CI 0.747–0.797). C# median 98.929 ns/op; paired current/C# **1.003** (0.945–1.030), inconclusive.
- `substring_span`: Dream 1.719 → 1.114 ns/op; paired current/baseline **0.713** (95% CI 0.671–0.735). C# median 1.513 ns/op; paired current/C# **0.740** (0.702–0.859), win.

The balanced 10% baseline regression gate reports 30 pass,
12 inconclusive and 0 regression rows.
The cyclic trial had 3 pass and 39 inconclusive rows. Code comparison expands LLVM
attributes and string constants: 38 of 41 benchmark entry functions have unchanged
instructions, constants and attributes. Only `bench_nbody`, `bench_substring_span` and
`bench_map_get_span` change. This evidence does not substitute for a performance gate.
Raw timings, stdout, optimized IR and assembly remain under `target/perf-two-fixes/`.

## Validation

Workspace debug and release builds, Clippy with `-D warnings`, and the complete workspace
suite passed. The suite used `RUST_TEST_THREADS=2`: parallel attempts hit the existing
10-second native-execution timeout in the leak diagnostic; the focused diagnostic and
final suite passed with no timeout relaxation. All 657 native goldens passed. Focused
release tests for clamping extremes, borrowed span loops and field-cache aliases also
passed with native/wasm output parity. New MIR tests cover disjoint and same-slot aliases,
shared objects, unknown calls, ownership effects, raw/global stores, source reassignment,
live-out values and the zero-trip guard.
