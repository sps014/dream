# Text capacity follow-up — 2026-10-12

The escaped-string builder now reserves for the remaining input, avoiding a growth after copying the initial unescaped run. The initial text campaign is preserved separately. This follow-up uses 20 alternating local macOS arm64 process rounds, two warmups and five measured passes, with uninstrumented timing and 95% confidence intervals. All checksums match and all rows pass the 10% hard gate. No dedicated-runner or cross-platform performance acceptance is claimed.

- `builder_double`: current/reference 0.566; 95% CI 0.553–0.577; pass.
- `json_escaped_256`: current/reference 0.313; 95% CI 0.311–0.315; pass.
- `json_string_256`: current/reference 0.249; 95% CI 0.246–0.253; pass.
- `json_string_8`: current/reference 0.772; 95% CI 0.756–0.779; pass.
- `json_string_8192`: current/reference 0.165; 95% CI 0.161–0.167; pass.

Raw samples and complete identity are under `target/perf-plan/text-reserve-timing`. The reference snapshot is `target/perf-plan/current-snapshot`; the current snapshot is `target/perf-plan/pools-snapshot`. Counters run separately.
