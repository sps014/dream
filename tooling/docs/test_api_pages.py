"""Regression checks for reference page headings and signature wrapping."""

import unittest

from api_pages import declaration_name, wrap


class PageTests(unittest.TestCase):
    def test_member_names(self):
        self.assertEqual(declaration_name("public class MemoryStream: Reader, Writer"), "MemoryStream")
        self.assertEqual(declaration_name("public constructor(bytes: byte[])"), "constructor")
        self.assertEqual(declaration_name("public get length(): long"), "length")
        self.assertEqual(declaration_name("public static async fun spawn<T>(work: fun(): T): Task<T>"), "spawn")
        self.assertEqual(declaration_name("public fun this[index: int]: T"), "this")
        self.assertEqual(declaration_name("public enum struct Shape"), "Shape")

    def test_fields_and_enum_choices(self):
        self.assertEqual(declaration_name("public start: int"), "start")
        self.assertEqual(declaration_name("Clear(value: T)"), "Clear")
        self.assertEqual(declaration_name("Global = 1"), "Global")
        self.assertEqual(declaration_name("Empty"), "Empty")

    def test_long_signatures_wrap_at_top_level_commas(self):
        signature = ("public async fun read_async(max_bytes: int, token: Option<CancellationToken> = "
                     "Option.None, limits: Map<string, int>): Result<byte[], IoError>")
        self.assertEqual(wrap(signature),
                         "public async fun read_async(\n    max_bytes: int,\n"
                         "    token: Option<CancellationToken> = Option.None,\n"
                         "    limits: Map<string, int>\n): Result<byte[], IoError>")

    def test_short_signatures_stay_on_one_line(self):
        self.assertEqual(wrap("public fun tell(): Result<long, IoError>"),
                         "public fun tell(): Result<long, IoError>")


if __name__ == "__main__":
    unittest.main()
