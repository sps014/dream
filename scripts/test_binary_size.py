import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("binary_size", Path(__file__).with_name("check-binary-size.py"))
binary_size = importlib.util.module_from_spec(spec)
spec.loader.exec_module(binary_size)


class BudgetTests(unittest.TestCase):
    def test_limits_are_inclusive(self):
        self.assertEqual(binary_size.check_budget(dict(binary_size.BUDGETS)), [])

    def test_each_artifact_is_bounded_independently(self):
        for name in binary_size.BUDGETS:
            sizes = dict(binary_size.BUDGETS)
            sizes[name] += 1
            self.assertEqual(len(binary_size.check_budget(sizes)), 1)

    def test_empty_artifacts_fail(self):
        self.assertEqual(len(binary_size.check_budget({"hello": 0, "core": 0})), 2)


if __name__ == "__main__":
    unittest.main()
