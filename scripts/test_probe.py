import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import probe_test as probe


def windows_default_read(path, encoding=None, errors=None):
    with path.open(encoding=encoding or "cp1252", errors=errors or "strict") as source:
        return source.read()


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


class FailureGoldenTests(unittest.TestCase):
    def node_case(self, expected_suffix, expected, outputs):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            case = root / "failure_fixture.dream"
            case.with_suffix(expected_suffix).write_text(expected, encoding="utf-8")
            destination = root / "target/probe-wasm/failure_fixture"
            destination.mkdir(parents=True)
            (destination / "failure_fixture.wasm").write_bytes(b"")
            with patch.object(probe, "root", root), patch.object(
                probe, "run_group", side_effect=outputs
            ) as run:
                result = probe.one_node(case)
                return result, run.call_count

    def test_node_compiler_failure_must_match_diagnostics(self):
        result, count = self.node_case(
            ".expected_error", "unknown symbol", [(1, "", "toolchain missing")]
        )
        self.assertEqual(result[1], "fail")
        self.assertIn("unknown symbol", result[2])
        self.assertEqual(count, 1)

    def test_node_trap_checks_message_location_and_exit_code(self):
        result, count = self.node_case(
            ".expected_trap", "boom\nfailure_fixture.dream:4\nexit code 1\n",
            [(0, "", ""), (1, "", "boom\n  at failure_fixture.dream:4")],
        )
        self.assertEqual(result, ("failure_fixture", "ok", ""))
        self.assertEqual(count, 2)

    def test_node_trap_rejects_wrong_location(self):
        result, _ = self.node_case(
            ".expected_trap", "boom\nfailure_fixture.dream:4\n",
            [(0, "", ""), (1, "", "boom\n  at failure_fixture.dream:5")],
        )
        self.assertEqual(result[1], "fail")

    def test_node_trap_checks_nonzero_program_status(self):
        result, _ = self.node_case(
            ".expected_trap", "exit code 2", [(0, "", ""), (2, "", "")]
        )
        self.assertEqual(result, ("failure_fixture", "ok", ""))
        result, _ = self.node_case(
            ".expected_trap", "exit code 3", [(0, "", ""), (2, "", "")]
        )
        self.assertEqual(result[1], "fail")

    def test_node_trap_rejects_success(self):
        result, _ = self.node_case(
            ".expected_trap", "boom", [(0, "", ""), (0, "boom", "")]
        )
        self.assertEqual(result[1:], ("fail", "expected trap"))

    def test_node_trap_rejects_timeout_even_after_expected_output(self):
        for code, stderr in [(-9, "boom"), (2, "boom\nprobe --node timeout")]:
            with self.subTest(code=code):
                result, _ = self.node_case(
                    ".expected_trap", "boom", [(0, "", ""), (code, "", stderr)]
                )
                self.assertEqual(result[1:], ("fail", "node timed out"))

    def test_native_trap_rejects_timeout(self):
        with tempfile.TemporaryDirectory() as directory:
            case = Path(directory) / "failure_fixture.dream"
            case.with_suffix(".expected_trap").write_text("timeout", encoding="utf-8")
            with patch.object(probe, "run_group", return_value=(-9, "timeout", "")):
                self.assertEqual(probe.one(case)[1:], ("fail", "run timed out"))


if __name__ == "__main__":
    unittest.main()
