#!/usr/bin/env python3
"""Cross-language measurements must reject unmatched work before publishing ratios."""
import copy
import unittest
from unittest.mock import patch

from benchmarks import language_report as report


class LanguageReportTests(unittest.TestCase):
    def test_timing_runs_reject_instrumentation_and_use_one_measured_pass(self):
        with patch.dict("os.environ", {}, clear=True):
            self.assertEqual(report.timing_environment("0"), {"DREAM_BENCH_SEED": "0", "DREAM_BENCH_PASSES": "1"})
        for key in ("DREAM_NATIVE_SANITIZE", "DREAM_RUNTIME_COUNTERS", "DREAM_BENCH_COUNTERS"):
            with self.subTest(key=key), patch.dict("os.environ", {key: "1"}, clear=True), self.assertRaises(ValueError):
                report.timing_environment("1")

    def samples(self):
        return {arm: [report.parse_rows(f"bench work ns_total={ns} iters=10 checksum=-42\nsink 1\n")
                      for ns in totals]
                for arm, totals in {"dream": [100, 200, 300], "rust": [200, 400, 600], "go": [50, 100, 150]}.items()}

    def test_fractional_ns_per_op_and_each_comparator_use_process_medians(self):
        row = report.summarize(self.samples())[0]
        self.assertEqual(row["measurements"]["dream"]["ns_per_op"], [10, 20, 30])
        self.assertEqual(row["comparisons"]["rust"], {"ratio": .5, "verdict": "Dream"})
        self.assertEqual(row["comparisons"]["go"], {"ratio": 2, "verdict": "Go"})
        self.assertEqual(report.parse_rows("bench fractional ns_total=3 iters=10 checksum=1")["fractional"]["ns_total"] / 10, .3)

    def test_duplicate_missing_malformed_and_nonpositive_rows_are_rejected(self):
        for text in ("sink 1", "bench broken", "bench bad ns_total=0 iters=1 checksum=1",
                     "bench bad ns_total=1 iters=0 checksum=1",
                     "bench bad ns_total=-1 iters=1 checksum=1",
                     "bench a ns_total=1 iters=1 checksum=1\nbench a ns_total=2 iters=1 checksum=1"):
            with self.subTest(text=text), self.assertRaises(ValueError):
                report.parse_rows(text)

    def test_checksum_and_iterations_must_match_within_and_between_arms(self):
        for arm in ("dream", "rust", "go"):
            for field in ("checksum", "iters"):
                samples = self.samples()
                samples[arm][1]["work"][field] += 1
                with self.subTest(arm=arm, field=field), self.assertRaisesRegex(ValueError, field):
                    report.summarize(samples)

    def test_missing_extra_and_incomplete_samples_are_rejected(self):
        for mode in ("missing", "extra", "incomplete"):
            samples = self.samples()
            if mode == "missing":
                samples["rust"][0].pop("work")
            elif mode == "extra":
                samples["rust"][0]["extra"] = copy.deepcopy(samples["rust"][0]["work"])
            else:
                samples["rust"].pop()
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                report.summarize(samples)

    def test_three_arm_order_rotates_without_mutating_the_arms(self):
        arms = ["dream", "rust", "go"]
        self.assertEqual([report.order_for(i, arms) for i in range(3)],
                         [["dream", "rust", "go"], ["rust", "go", "dream"], ["go", "dream", "rust"]])
        self.assertEqual(arms, ["dream", "rust", "go"])

    def test_render_includes_both_ratios_and_semantic_qualifications(self):
        text = report.markdown({"suite": "micro", "against": ["rust", "go"], "rounds": 3,
                                "seed": "1", "platform": "test host", "rows": report.summarize(self.samples())})
        for phrase in ("Dream/Rust", "Dream/Go", "Tokio", "UTF-16", "GC", "not a statistical regression gate"):
            self.assertIn(phrase, text)


if __name__ == "__main__":
    unittest.main()
