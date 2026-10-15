"""Regression checks for declaration coverage, including visibility and trivia."""

import unittest

from api_inventory import declarations


class InventoryTests(unittest.TestCase):
    def test_private_owner_is_excluded(self):
        source = "class Hidden {\n    public fun helper(): int { return 1; }\n}\npublic class Visible {\n    public fun answer(): int { return 42; }\n}"
        signatures = [entry["signature"] for entry in declarations(source)]
        self.assertEqual(signatures, ["public class Visible", "public fun answer(): int"])

    def test_shared_owner_visibility(self):
        source = "shared internal class Hidden {\n    public fun helper(): int { return 0; }\n}\n// A shared endpoint.\nshared public class Sender<T> {\n    // Sends a value.\n    public fun send(value: T): bool { return true; }\n}"
        entries = declarations(source)
        self.assertEqual([entry["signature"] for entry in entries], ["shared public class Sender<T>", "public fun send(value: T): bool"])
        self.assertEqual(entries[0]["description"], "A shared endpoint.")

    def test_interface_requirements_and_comments(self):
        source = "public interface Reader {\n    // Reads one value.\n    fun read(): int;\n}"
        entries = declarations(source)
        self.assertEqual(entries[-1]["signature"], "fun read(): int")
        self.assertEqual(entries[-1]["description"], "Reads one value.")

    def test_interface_receiver_and_async_qualifiers(self):
        source = "public interface Streams {\n    unique fun read(): int;\n    async unique fun write(): int;\n    borrow fun tell(): int;\n}"
        signatures = [entry["signature"] for entry in declarations(source)]
        self.assertEqual(signatures[1:], ["unique fun read(): int", "async unique fun write(): int", "borrow fun tell(): int"])

    def test_multiline_parameters_and_string_delimiters(self):
        source = 'public class Format {\n    public fun render(\n        pattern: string = "{x};",\n        action: fun(int): string\n    ): string { return pattern; }\n}'
        self.assertEqual(declarations(source)[-1]["signature"], 'public fun render( pattern: string = "{x};", action: fun(int): string ): string')

    def test_enum_choices(self):
        source = "public enum Choice {\n    First,\n    Second(int),\n}"
        self.assertEqual([entry["signature"] for entry in declarations(source)],
                         ["public enum Choice", "First", "Second(int)"])

    def test_interface_requirement_modifiers(self):
        source = "public interface Cursor<T> {\n    borrow fun iterator(): Iterator<T>;\n    async unique fun next(): Option<T>;\n    unique fun close(): void;\n}"
        self.assertEqual([entry["signature"] for entry in declarations(source)],
                         ["public interface Cursor<T>", "borrow fun iterator(): Iterator<T>",
                          "async unique fun next(): Option<T>", "unique fun close(): void"])


if __name__ == "__main__":
    unittest.main()
