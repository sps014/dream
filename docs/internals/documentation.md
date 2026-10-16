# Maintain the documentation

The user guides explain how to write Dream programs. The contributor handbook explains how Dream itself works. Keep implementation details in the handbook unless a user needs them to make a decision.

## Write for the reader

Keep each guide focused on a user task. Explain its inputs, setup, result, and failure
handling on the page. Include imports in complete programs. Use related-guide links
for optional detail; do not require a repository checkout or a trip to sample folders.

Use the shared formatter for Dream fences. Never put a nonempty function body or
several statements on one line. Separate logical phases with a blank line, and split
large workflows into explained steps rather than an uninterrupted code dump.

Every public stdlib declaration needs an attached contract. Use `///` for purpose,
parameters and units, defaults, results, failures, and restrictions. Ordinary `//`
comments can describe implementation invariants separately; these must not replace
or leak into the user-facing contract. The inventory prefers doc comments over
implementation comments and recognizes wrapped attributes and signatures.

## When an API changes

Read its declaration and implementation before updating the reference. Check its parameters, defaults, result, failures, and platform restrictions. Update the relevant usage guide and API lookup page together.

The public declaration inventory covers registered library sources with explicitly public declarations, interface requirements, and enum choices. It excludes private and internal declarations. Language-provided operations and imported external declarations still need manual review in their feature guides; the inventory cannot prove their behavior or availability.

```sh
python tooling/docs/api_inventory.py
python -m unittest discover -s tooling/docs -p "test_*.py"
```

If the source changed, update the documentation first. Then refresh the reviewed inventory with `python tooling/docs/api_inventory.py --write`. The check rejects a changed source inventory, an empty description, or a missing signature on its assigned page. It does not grade the quality of an explanation.

## Check complete examples

`tooling/docs/example-cases.json` lists complete, self-contained examples and their expected output. Each entry identifies its page and Dream code-block position. A multi-file entry can list `files` with their code-block positions and select an `entry` filename. Add examples to that list when they can run without external setup.

```sh
cargo build --workspace
python tooling/docs/check_examples.py
```

Use `--dream <path>` to select a compiler. The check writes temporary programs and build output under `target/docs-examples/`. It runs the code from the documentation rather than keeping a separate copy.

Integration examples require their documented environment. Record which environment was checked and report unavailable checks honestly. Signature fragments and deliberately invalid examples are not runnable programs.

## Add a page or section

The navigation, the section directories, and the previous/next links are generated from folders and front matter. You never edit a central page list.

- **New page:** add a Markdown file to the folder for its topic. It appears at the end of that section until you place it in the folder's `.nav.yml`.
- **New category or section:** add a folder with an `index.md` and a `.nav.yml` that sets its `title`. Its parent's directory picks it up automatically.
- **Order:** list file and folder names in the `nav:` of the folder's `.nav.yml`. Unlisted entries still appear after the listed ones.

Every page listed in a directory needs front matter:

```yaml
---
title: Async / Await          # short name for directories and the sidebar
summary: Write work that waits without blocking, using futures and `await`.
icon: material/timer-sand     # any Material Design icon name
status: stable                # language pages: stable, experimental, or planned
targets: [native, web, node]  # language pages: where the feature works
api: reference/api/system-task.md   # optional "API" link in directories
related: [language/concurrency/tasks.md]  # optional related-guide strip
---
```

`status` and `targets` are required for pages under `docs/language/`, and section index pages need `summary` and `icon`. `tooling/docs/site_hook.py` reports a missing or unknown value as a build warning, which `mkdocs build --strict` turns into a failure. Set `listed: false` on an index page to keep its section out of its parent's directory.

An index page lists its children where it contains `<!-- pages -->`. The home page shows the top-level sections as cards; every other index page shows a directory with one row per page. `<!-- pages grouped -->` gives each child section its own column, as on the Language and Standard library pages. Section pages keep the sidebar and hide only the table of contents with `hide: [toc]`. `icon` appears on the home page cards.

Language feature pages follow the shape in `docs/language/_template.md`: front matter, a title and one-paragraph introduction, an **At a glance** block with the key syntax, then sections for usage, rules and edge cases, and examples. Keep the glance snippet short and copy it from code the page already explains. Write it as an indented fence inside the `!!! glance` block so `check_examples.py` block positions do not change.

Use `template: tour.html` for a step-by-step walkthrough. Wrap each step in `<div class="tour-step" markdown>`; on wide screens its code fence stays beside the explanation.

## Check the site

```sh
python -m pip install -r docs/requirements-docs.txt
mkdocs build --strict
mkdocs serve
```

Review the rendered learning path, a reference page, a recipe, and the changed navigation. Check links, code formatting, tables on narrow screens, and the path from an overview to a detailed page. Use descriptive link text and explain diagrams in nearby prose.

## Keep pages focused

Give a page one task or closely related API group. Move a substantial separate workflow to a new page and link it from an overview. Update links when moving a section; do not leave two full copies. Use `dream` fences for Dream code and identify setup requirements before an example.
