#!/usr/bin/env python3
"""Check adapter freshness under local and CI terminal settings."""
import os
from pathlib import Path
import subprocess
import sys
import unittest


ROOT = Path(__file__).resolve().parents[1]


class CliParserTests(unittest.TestCase):
    def test_freshness_is_independent_of_cargo_terminal_color(self):
        """Cargo's colored duplicate markers must never affect the source fingerprint."""
        for color in ["never", "always"]:
            with self.subTest(color=color):
                result = subprocess.run(
                    [sys.executable, "scripts/build-cli-parser.py", "--check"],
                    cwd=ROOT,
                    env={**os.environ, "CARGO_TERM_COLOR": color},
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
