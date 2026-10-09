#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import tempfile
import unittest


spec = importlib.util.spec_from_file_location("remarks", Path(__file__).with_name("llvm-remarks.py"))
remarks = importlib.util.module_from_spec(spec)
spec.loader.exec_module(remarks)


class RemarkTests(unittest.TestCase):
    def test_groups_reasons_without_losing_clobbers_or_callees(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "remarks.yaml"
            document = """--- !Missed
Pass: gvn
Name: LoadClobbered
Function: bench_scan
Args:
  - String: 'load is clobbered by '
  - ClobberedBy: dream_retain
  - DebugLoc: {File: 'source.dream', Line: 2, Column: 1}
...
"""
            path.write_text(document * 2 + """--- !Passed
Pass: inline
Name: Inlined
Function: bench_scan
Args:
  - Callee: char_at
...
""")
            rows = remarks.summarize([path])
            self.assertEqual(len(rows), 2)
            clobber = next(row for row in rows if row['reason'] == 'LoadClobbered')
            self.assertEqual(clobber['count'], 2)
            self.assertIn('ClobberedBy=dream_retain', clobber['details'])
            self.assertTrue(any('Callee=char_at' in row['details'] for row in rows))
            path.write_text('')
            self.assertEqual(remarks.summarize([path]), [])


if __name__ == '__main__':
    unittest.main()
