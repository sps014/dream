#!/usr/bin/env python3
"""Check relative budgeting independently of clocks and artifact sizes."""
import unittest
from check_compile_budget import compare, sample


class CompileBudgetTests(unittest.TestCase):
    def test_medians_ignore_a_single_outlier(self):
        baseline = [{'wall_seconds': n, 'peak_resident_bytes': 100} for n in [10, 10, 1000]]
        candidate = [{'wall_seconds': n, 'peak_resident_bytes': 120} for n in [11, 11, 5000]]
        metrics = compare(baseline, candidate, 1.25, 1.20)
        self.assertTrue(all(value['passed'] for value in metrics.values()))

    def test_time_and_memory_fail_independently(self):
        before = [{'wall_seconds': 10, 'peak_resident_bytes': 100}]
        for key in ['wall_seconds', 'peak_resident_bytes']:
            after = [dict(before[0], **{key: before[0][key] * 2})]
            metrics = compare(before, after, 1.25, 1.20)
            self.assertFalse(metrics[key]['passed'])
            self.assertEqual(sum(not value['passed'] for value in metrics.values()), 1)

    def test_sample_scales_and_calls_every_generated_function(self):
        for functions in [1, 3, 17]:
            text = sample(functions)
            self.assertIn(f'System.println(stage_{functions - 1}(7))', text)
            for index in range(functions):
                self.assertIn(f'fun stage_{index}(', text)
                if index:
                    self.assertIn(f'stage_{index - 1}(seed)', text)


if __name__ == '__main__':
    unittest.main()
