import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import probe_test as probe


def windows_default_read(path, encoding=None, errors=None):
    return path.read_bytes().decode(encoding or "cp1252", errors or "strict")


class GoldenEncodingTests(unittest.TestCase):
    def test_native_golden_is_utf8_under_a_windows_default_encoding(self):
        with tempfile.TemporaryDirectory() as directory:
            case = Path(directory) / "unicode_fixture.dream"
            case.with_suffix(".expected").write_text("Straße\nhéllo\n", encoding="utf-8")
            with patch.object(Path, "read_text", windows_default_read), patch.object(
                probe, "run_group", return_value=(0, "Straße\nhéllo\n", "")
            ):
                self.assertEqual(probe.one(case), ("unicode_fixture", "ok", ""))

    def test_node_golden_is_utf8_under_a_windows_default_encoding(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            case = root / "unicode_fixture.dream"
            case.with_suffix(".expected").write_text("Straße\nhéllo\n", encoding="utf-8")
            destination = root / "target/probe-wasm/unicode_fixture"
            destination.mkdir(parents=True)
            (destination / "unicode_fixture.wasm").write_bytes(b"")
            with patch.object(probe, "root", root), patch.object(
                Path, "read_text", windows_default_read
            ), patch.object(
                probe, "run_group", side_effect=[(0, "", ""), (0, "Straße\nhéllo\n", "")]
            ):
                self.assertEqual(probe.one_node(case), ("unicode_fixture", "ok", ""))


if __name__ == "__main__":
    unittest.main()
