# Dream performance campaign: text — 2026-10-12

Local macOS arm64 exploration: 20 alternating process rounds per compiler, two warmup rounds, five measured passes per process, uninstrumented timing, bootstrap 95% confidence intervals. This is not dedicated-runner or cross-platform acceptance. Checksums match in every workload.

- `builder_double`: current/reference 0.570, 95% CI 0.558–0.577; pass.
- `json_escaped_256`: current/reference 0.378, 95% CI 0.373–0.380; pass.
- `json_string_256`: current/reference 0.251, 95% CI 0.245–0.256; pass.
- `json_string_8`: current/reference 0.741, 95% CI 0.732–0.758; pass.
- `json_string_8192`: current/reference 0.158, 95% CI 0.154–0.162; pass.

Peak process RSS: reference 1523712 bytes, current 1490944 bytes. RSS is process-level and does not isolate allocator retention.

Raw samples, binaries, fixture copies and measurement identity are preserved under `target/perf-plan/text-timing`. Compiler and matching runtime snapshots are under `target/perf-plan/baseline` (collections reference) and `target/perf-plan/current-snapshot` (text reference). The checked-in JSON contains process samples and measurement metadata.

The targeted changes are direct owned construction of unescaped JSON strings, bulk quote/backslash scanning, and builder double formatting without a temporary string. CLI output additionally uses bounded conversion storage; these timing rows do not isolate its cost. Formatting continues to use the existing six-significant-digit conversion. Unicode, mutable slices, builder snapshots, malformed documents and special floating-point values are covered by correctness tests.

Executable sizes: baseline 127184 bytes, current 127328 bytes.
