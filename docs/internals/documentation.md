# Maintain the documentation

The user guides explain how to write Dream programs. The contributor handbook explains how Dream itself works. Keep implementation details in the handbook unless a user needs them to make a decision.

## When an API changes

Read its declaration and implementation before updating the reference. Check its parameters, defaults, result, failures, and platform restrictions. Update the relevant usage guide and API lookup page together.

The public declaration inventory covers registered library sources with explicitly public declarations, interface requirements, and enum choices. It excludes private and internal declarations. Language-provided operations and imported external declarations still need manual review in their feature guides; the inventory cannot prove their behavior or availability.

```sh
python tooling/docs/api_inventory.py
python -m unittest discover -s tooling/docs -p "test_*.py"
```

If the source changed, update the documentation first. Then refresh the reviewed inventory with `python tooling/docs/api_inventory.py --write`. The check rejects a changed source inventory or a missing signature on its assigned page. It does not grade the quality of an explanation.

## Check complete examples

`tooling/docs/example-cases.json` lists complete, self-contained examples and their expected output. Each entry identifies its page and Dream code-block position. Add examples to that list when they can run without a network service, a desktop window, a GPU, or other external setup.

```sh
cargo build --workspace
python tooling/docs/check_examples.py
```

Use `--dream <path>` to select a compiler. The check writes temporary programs and build output under `target/docs-examples/`. It runs the code from the documentation rather than keeping a separate copy.

Integration examples require their documented environment. Record which environment was checked and report unavailable checks honestly. Signature fragments and deliberately invalid examples are not runnable programs.

## Check the site

```sh
python -m pip install -r docs/requirements-docs.txt
mkdocs build --strict
mkdocs serve
```

Review the rendered learning path, a reference page, a recipe, and the changed navigation. Check links, code formatting, tables on narrow screens, and the path from an overview to a detailed page. Use descriptive link text and explain diagrams in nearby prose.

## Keep pages focused

Give a page one task or closely related API group. Move a substantial separate workflow to a new page and link it from an overview. Update links when moving a section; do not leave two full copies. Use `dream` fences for Dream code and identify setup requirements before an example.
