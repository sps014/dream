"""Regression checks for declaration coverage, including visibility and trivia."""

import unittest

from api_inventory import declarations


class InventoryTests(unittest.TestCase):
    def test_private_owner_is_excluded(self):
        source = "class Hidden {\n    public fun helper(): int { return 1; }\n}\npublic class Visible {\n    public fun answer(): int { return 42; }\n}"
        signatures = [entry["signature"] for entry in declarations(source)]
        self.assertEqual(signatures, ["public class Visible", "public fun answer(): int"])

    def test_interface_requirements_and_comments(self):
        source = "public interface Reader {\n    // Reads one value.\n    fun read(): int;\n}"
        entries = declarations(source)
        self.assertEqual(entries[-1]["signature"], "fun read(): int")
        self.assertEqual(entries[-1]["description"], "Reads one value.")

    def test_multiline_parameters_and_string_delimiters(self):
        source = 'public class Format {\n    public fun render(\n        pattern: string = "{x};",\n        action: fun(int): string\n    ): string { return pattern; }\n}'
        self.assertEqual(declarations(source)[-1]["signature"], 'public fun render( pattern: string = "{x};", action: fun(int): string ): string')

    def test_enum_choices(self):
        source = "public enum Choice {\n    First,\n    Second(int),\n}"
        self.assertEqual([entry["signature"] for entry in declarations(source)],
                         ["public enum Choice", "First", "Second(int)"])


if __name__ == "__main__":
    unittest.main()
