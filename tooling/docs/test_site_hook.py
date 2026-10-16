"""Section directories, home cards, and front matter validation for the documentation site hook."""

import unittest

from mkdocs.config.defaults import MkDocsConfig
from mkdocs.structure.files import File, Files
from mkdocs.structure.nav import Section
from mkdocs.structure.pages import Page

from site_hook import card_items, render_hub, validate_meta

CONFIG = MkDocsConfig()
CONFIG.load_dict({"site_name": "Test"})


def page(path, title, **meta):
    file = File(path, src_dir="/docs", dest_dir="/site", use_directory_urls=True)
    result = Page(title, file, CONFIG)
    result.meta = meta
    return result


def section(title, *children):
    result = Section(title, list(children))
    for child in children:
        child.parent = result
    return result


def language_tree():
    hub = page("language/index.md", "Language", summary="Every feature.", icon="material/code-braces")
    basics_index = page("language/basics/index.md", "Basics", summary="Building blocks.", icon="material/alphabetical-variant")
    variables = page("language/basics/variables.md", "Variables", summary="Name values with `let`.",
                     icon="material/variable", status="stable", targets=["native", "web", "node"])
    basics = section("Basics", basics_index, variables)
    language = section("Language", hub, basics)
    return hub, basics, variables, language


class ValidationTests(unittest.TestCase):
    def test_complete_feature_page_is_valid(self):
        meta = {"summary": "s", "status": "experimental", "targets": ["native", "node"]}
        self.assertEqual(validate_meta("language/basics/variables.md", meta), [])

    def test_feature_page_needs_summary_status_and_known_targets(self):
        problems = validate_meta("language/basics/variables.md", {"status": "beta", "targets": ["wasm"]})
        self.assertEqual(len(problems), 3)

    def test_index_page_needs_icon_but_not_status(self):
        self.assertEqual(validate_meta("language/basics/index.md", {"summary": "s"}, is_index=True), ["missing `icon`"])

    def test_pages_outside_language_are_not_validated(self):
        self.assertEqual(validate_meta("tooling/commands.md", {}), [])


class HubTests(unittest.TestCase):
    def test_hub_lists_siblings_and_skips_itself(self):
        hub, basics, _, _ = language_tree()
        self.assertEqual(card_items(hub, []), [basics])

    def test_homepage_lists_top_level_sections_except_hidden_ones(self):
        home = page("index.md", "Home")
        _, _, _, language = language_tree()
        internals = section("Contributors", page("internals/README.md", "Contributors", listed=False))
        self.assertEqual(card_items(home, [home, language, internals]), [language])

    def test_leaf_page_has_no_cards(self):
        _, _, variables, _ = language_tree()
        self.assertIsNone(card_items(variables, []))

    def test_section_card_links_to_its_index_and_counts_pages(self):
        hub, basics, _, _ = language_tree()
        html = render_hub(hub, [basics], grouped=False)
        self.assertIn('href="basics/"', html)
        self.assertIn("Building blocks.", html)
        self.assertIn("1 page", html)

    def test_grouped_hub_renders_heading_then_feature_rows(self):
        hub, basics, _, _ = language_tree()
        html = render_hub(hub, [basics], grouped=True)
        self.assertLess(html.index("<h2"), html.index("Variables"))
        self.assertIn("<code>let</code>", html)
        self.assertIn('data-targets="native web node"', html)

    def test_only_the_homepage_renders_cards(self):
        hub, basics, _, language = language_tree()
        home = page("index.md", "Home", summary="Docs.")
        self.assertIn("dream-card", render_hub(home, [language], grouped=False))
        section_html = render_hub(hub, [basics], grouped=False)
        self.assertNotIn("dream-card", section_html)
        self.assertIn("dream-index__item", section_html)

    def test_rows_mark_only_restricted_targets(self):
        hub, _, _, _ = language_tree()
        everywhere = page("language/a.md", "A", summary="s", targets=["native", "web", "node"])
        browser = page("language/b.md", "B", summary="s", targets=["web"])
        html = render_hub(hub, [everywhere, browser], grouped=False)
        self.assertNotIn(">Native<", html)
        self.assertIn("Browser only", html)

    def test_new_folder_becomes_a_card_without_configuration(self):
        hub, basics, _, _ = language_tree()
        new_index = page("language/effects/index.md", "Effects", summary="A new topic.", icon="material/flash")
        effects = section("Effects", new_index, page("language/effects/intro.md", "Intro", summary="First page."))
        html = render_hub(hub, [basics, effects], grouped=False)
        self.assertIn('href="effects/"', html)
        self.assertIn("A new topic.", html)

    def test_api_link_resolves_through_files(self):
        hub, _, _, _ = language_tree()
        api_file = File("reference/api/system-core.md", src_dir="/docs", dest_dir="/site", use_directory_urls=True)
        card = page("language/builtins.md", "Built-ins", summary="Console.", api="reference/api/system-core.md")
        html = render_hub(hub, [card], grouped=False, files=Files([api_file]))
        self.assertIn('href="../reference/api/system-core/"', html)


if __name__ == "__main__":
    unittest.main()
