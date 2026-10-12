# Ready queue block reuse — 2026-10-12

The targeted cost is per-entry allocation above the existing 64-entry TLS cache. Overflow entries now allocate in blocks of 64. Native run-loop teardown returns spare blocks to a process cache bounded at eight blocks; wasm returns them to its guest heap. Publication caching was added in the same runtime snapshot but these workloads do not publish worker graphs.

The initial 20-round local macOS arm64 run had three inconclusive small-burst rows. Its predefined follow-up waited for compiler processes to become idle and used 20 alternating rounds, two warmup rounds and ten measured passes. All five rows pass the 10% hard gate, with matching checksums. This remains local exploratory evidence, not dedicated-runner or native x86-64 hardware acceptance.

- `ready_burst_4096`: current/reference 0.579, 95% CI 0.573–0.585; pass.
- `ready_burst_513`: current/reference 0.558, 95% CI 0.554–0.566; pass.
- `ready_burst_64`: current/reference 1.004, 95% CI 0.978–1.010; pass.
- `ready_burst_65`: current/reference 0.972, 95% CI 0.944–0.977; pass.
- `ready_burst_8`: current/reference 0.996, 95% CI 0.981–1.018; pass.

Raw initial/follow-up samples are preserved in `target/perf-plan/ready-timing` and `target/perf-plan/ready-followup`. Both arms use the identical compiler under `target/perf-plan/pools-snapshot`. Matching runtimes are under `target/perf-plan/heap-thread-snapshot` and `target/perf-plan/pools-snapshot`.

Correctness includes overflow cancellation natively and on Node, FIFO ordering, cache boundaries, 4096-entry bursts, 100 recurring sessions, concurrent schedulers and AddressSanitizer/UBSan. Standalone native x86-64 ready/output harnesses pass through Rosetta; this does not establish native x86-64 performance.

Separate instrumented samples count actual ready-node/block allocations per measured pass:

- `ready_burst_4096`: 80640 → 0.
- `ready_burst_513`: 44900 → 0.
- `ready_burst_64`: 0 → 0.
- `ready_burst_65`: 500 → 0.
- `ready_burst_8`: 0 → 0.
