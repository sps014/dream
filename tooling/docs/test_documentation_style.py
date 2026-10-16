"""Keep readable examples a requirement of the documentation build."""

import re
import unittest
from pathlib import Path

from api_inventory import mask

ROOT = Path(__file__).resolve().parents[2]


def cramped_lines(code):
    bad = []
    depth = 0
    for number, line in enumerate(mask(code).splitlines(), 1):
        statements = 0
        for char in line:
            if char in "([":
                depth += 1
            elif char in ")]":
                depth = max(0, depth - 1)
            elif char == ";" and depth == 0:
                statements += 1
        if statements > 1 or re.search(r"\{[^{}]*;[^{}]*\}", line):
            bad.append(number)
    return bad


class DocumentationStyleTests(unittest.TestCase):
    def test_strings_comments_and_for_headers_are_not_statements(self):
        self.assertEqual(cramped_lines('let s = "{ a; b; }"; // c; d;\nfor (let i = 0; i < 3; i++) {\n    run();\n}'), [])

    def test_compact_bodies_and_multiple_statements_are_rejected(self):
        self.assertEqual(cramped_lines('fun hello() { return "hello"; }\nlet x = 1; use(x);'), [1, 2])

    def test_documentation_examples_have_readable_bodies(self):
        failures = []
        for path in sorted((ROOT / "docs").rglob("*.md")):
            source = path.read_text(encoding="utf-8")
            for block in re.finditer(r"(?m)^[ \t]*```dream\n(.*?)^[ \t]*```", source, re.S):
                first_line = source.count("\n", 0, block.start(1))
                for line in cramped_lines(block[1]):
                    failures.append(f"{path.relative_to(ROOT)}:{first_line + line}")
        self.assertEqual(failures, [], "Expand bodies and separate statements:\n" + "\n".join(failures))
