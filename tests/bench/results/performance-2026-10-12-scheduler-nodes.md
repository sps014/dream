# Timer and foreign node blocks, 2026-10-12

The targeted cost is allocating/freeing a raw node for every timer and foreign completion.
Separate pools allocate 64 entries per block. Native spare caches retain at most eight blocks
per pool when idle; wasm timer blocks return to its guest heap. Slots initialize lazily, and
blocks retire only after their last active entry returns. Foreign nodes stay protected by the
pool lock. Completion claiming, publication and scheduler queue synchronization are unchanged.

The first timer fixture created lazy `Time.sleep` futures and awaited them sequentially. Its
samples are preserved in `target/perf-plan/timer-nodes-timing`, but they do not exercise an
active timer burst. The final fixture explicitly starts every future before awaiting results.
Its 20 alternating rounds, two warmups and five passes all pass the 10% gate:

- 1 timer: ratio 0.927, 95% CI 0.916–0.951.
- 8 timers: ratio 0.922, 95% CI 0.908–0.944.
- 64 timers: ratio 0.871, 95% CI 0.857–0.878.
- 513 timers: ratio 0.964, 95% CI 0.952–0.975.
- 4096 timers: ratio 0.962, 95% CI 0.947–0.968.

Separate instrumented runs record timer bookkeeping allocations per measured pass:
1: 5000 → 0; 8: 8000 → 0; 64: 6400 → 0; 513: 10260 → 20; 4096: 12288 → 168.
Guest allocation and ARC event counts remain equal. The reference counter snapshot adds only
attribution at the old allocation sites and the appended counter IDs; timing uses the preserved
uninstrumented reference runtime. Both arms use the same preserved compiler.

All five application workloads also pass the 10% gate: JSON service, log processing, graph
paths, async fanout and Task parallelism. A standalone foreign-node harness verifies no new
allocations during 40,000 recurring concurrent operations after warmup. Native sanitizers,
foreign completion lifecycle checks and native/Node cancellation cases pass. These are local
results, not dedicated-runner or native x86-64 hardware acceptance.

An indexed deadline heap and scheduler-owned foreign synchronization remain separate campaigns.
