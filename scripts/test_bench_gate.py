#!/usr/bin/env python3
"""Regression decisions must never turn invalid data into a passing gate."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

import bench_gate


class GateTests(unittest.TestCase):
    def test_two_arm_order_is_balanced(self):
        spec = importlib.util.spec_from_file_location('comparison', Path(__file__).with_name('bench-compare.py'))
        comparison = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(comparison)
        arms = ['baseline', 'current']
        orders = [comparison.order_for(i, arms) for i in range(20)]
        self.assertEqual(sum(order == arms for order in orders), 10)
        self.assertEqual(sum(order == arms[::-1] for order in orders), 10)
        self.assertEqual(arms, ['baseline', 'current'])
        source = comparison.baseline_source(comparison.BENCH.read_text())
        self.assertNotIn('counters_on', source)
        self.assertNotIn('Debug.runtime_counter', source)

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.binary = self.root / 'program'
        self.binary.write_bytes(b'immutable benchmark')
        self.identity = {'passes': 5, 'names': ['work']}
        self.samples = [dict(name='work', arm=arm, round=r, pass_index=p,
                             ns_total=1000, ns_per_op=value, sink=123)
                        for arm, value in [('baseline', 100), ('current', 100)]
                        for r in range(10) for p in range(5)]
        self.rss = {arm: [1000] * 10 for arm in ('baseline', 'current')}
        self.path = self.root / 'reference.json'
        tools = {f'{name}_sha256': 'a' * 64 for name in ('clang', 'opt', 'llc', 'llvm-link', 'current', 'baseline', 'baseline_artifact')}
        tools.update({f'{name}_version': 'pinned' for name in ('clang', 'opt', 'llc', 'llvm-link')})
        tools['llvm'] = 'pinned'
        bench_gate.save(self.path, self.root, self.identity, [str(self.binary)],
                        self.samples, 'baseline', 5, 10, self.rss, tools)
        self.record, _ = bench_gate.load(self.path, self.identity)

    def test_paired_decisions(self):
        for ratio, status in [(1.0, 0), (1.2, 1)]:
            samples = copy.deepcopy(self.samples)
            for sample in samples:
                if sample['arm'] == 'current':
                    sample['ns_per_op'] *= ratio
            self.assertEqual(bench_gate.evaluate(samples, self.record, 5, 10, self.rss, .1)[0], status)
        samples = copy.deepcopy(self.samples)
        for sample in samples:
            if sample['arm'] == 'current':
                sample['ns_per_op'] *= 1 if sample['round'] < 5 else 1.2
        self.assertEqual(bench_gate.evaluate(samples, self.record, 5, 10, self.rss, .1)[0], 2)

    def test_partial_duplicate_zero_and_missing_rows(self):
        variants = [self.samples[:-1], self.samples + [self.samples[-1]],
                    [s for s in self.samples if s['arm'] != 'current']]
        zero = copy.deepcopy(self.samples)
        zero[-1]['ns_per_op'] = 0
        variants.append(zero)
        for samples in variants:
            with self.subTest(samples=len(samples)), self.assertRaises(ValueError):
                bench_gate.evaluate(samples, self.record, 5, 10, self.rss, .1)

    def test_reference_integrity(self):
        mutations = [lambda r: r.update(version=1), lambda r: r.update(samples=[]),
                     lambda r: r.update(peak_rss=[1000]),
                     lambda r: r.update(confidence_intervals={'work': [0, 1]}),
                     lambda r: r.update(tools={})]
        for mutate in mutations:
            record = copy.deepcopy(self.record)
            mutate(record)
            self.path.write_text(json.dumps(record))
            with self.assertRaises(ValueError):
                bench_gate.load(self.path, self.identity)
        self.path.write_text(json.dumps(self.record))
        with self.assertRaises(ValueError):
            bench_gate.load(self.path, {'passes': 5, 'names': ['changed']})
        Path(self.record['reference']['binary']).write_bytes(b'changed')
        with self.assertRaises(ValueError):
            bench_gate.load(self.path, self.identity)

    def test_malformed_sample_fields_never_pass(self):
        for field, value in [('ns_per_op', True), ('ns_per_op', '100'),
                             ('ns_per_op', float('nan')), ('ns_total', False),
                             ('round', False), ('pass_index', True),
                             ('name', None), ('sink', '123')]:
            samples = copy.deepcopy(self.samples)
            samples[-1][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                bench_gate.evaluate(samples, self.record, 5, 10, self.rss, .1)
        for sample in [None, [], {}, {'arm': 'current'}]:
            with self.subTest(sample=sample), self.assertRaises(ValueError):
                bench_gate.process_samples([sample], 'current', 5, 10)

    def test_memory_regression_and_incomplete_samples(self):
        rss = copy.deepcopy(self.rss)
        rss['current'][0] = 1200
        self.assertEqual(bench_gate.evaluate(self.samples, self.record, 5, 10, rss, .1)[0], 1)
        rss['baseline'].pop()
        with self.assertRaises(ValueError):
            bench_gate.evaluate(self.samples, self.record, 5, 10, rss, .1)

    def test_result_sink_mismatch(self):
        samples = copy.deepcopy(self.samples)
        for sample in samples:
            if sample['arm'] == 'current':
                sample['sink'] = 456
        with self.assertRaises(ValueError):
            bench_gate.evaluate(samples, self.record, 5, 10, self.rss, .1)


if __name__ == '__main__':
    unittest.main()
