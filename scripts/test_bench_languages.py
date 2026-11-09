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
                for arm, totals in {"dream": [100000, 200000, 300000], "rust": [200000, 400000, 600000], "go": [50000, 100000, 150000]}.items()}

    def test_fractional_ns_per_op_and_each_comparator_use_process_medians(self):
        row = report.summarize(self.samples())[0]
        self.assertEqual(row["measurements"]["dream"]["ns_per_op"], [10000, 20000, 30000])
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

    def test_short_samples_and_extra_spawning_do_not_get_winner_verdicts(self):
        samples = self.samples()
        samples["rust"][0]["work"]["ns_total"] = 1
        self.assertEqual(report.summarize(samples)[0]["comparisons"]["rust"]["verdict"], "too short")
        samples = {arm: [{"async_poll": process["work"]} for process in processes]
                   for arm, processes in self.samples().items()}
        self.assertEqual(report.summarize(samples)[0]["comparisons"]["go"]["verdict"], "not equivalent")
        queued = {arm: [{"scheduler_queue": process["async_poll"]} for process in processes]
                  for arm, processes in samples.items()}
        self.assertEqual(report.summarize(queued)[0]["comparisons"]["go"]["verdict"], "not equivalent")
        self.assertEqual(report.summarize(queued)[0]["comparisons"]["rust"]["verdict"], "Dream")

    def test_both_suites_have_both_comparators_and_runtime_rows_are_identified(self):
        for _, arms in report.SUITES.values():
            self.assertEqual(set(arms), {"rust", "go"})
        self.assertEqual(report.workload_contract("async_fanout")[0], "runtime")
        self.assertEqual(report.workload_contract("byte_scan")[0], "representation")

    def test_summary_groups_timings_then_ratios_and_skips_methodology(self):
        text = report.markdown({"suite": "micro", "against": ["rust", "go"], "rounds": 3,
                                "seed": "1", "platform": "test host", "rows": report.summarize(self.samples())})
        self.assertIn("| Bench | Iterations | Dream ns/op | Rust ns/op | Go ns/op | Dream/Rust | Dream/Go | vs Rust | vs Go |", text)
        self.assertIn("0.90–1.10 is a tie", text)
        self.assertLess(text.count("\n"), 12)
        for phrase in ("Tokio", "UTF-16", "report.json", "statistical regression gate"):
            self.assertNotIn(phrase, text)
        queued = report.summarize({arm: [{"async_poll": process["work"]} for process in processes]
                                   for arm, processes in self.samples().items()})
        self.assertIn("not scored", report.markdown({"suite": "micro", "against": ["rust", "go"], "rounds": 3,
                                                     "seed": "1", "rows": queued}))


if __name__ == "__main__":
    unittest.main()
