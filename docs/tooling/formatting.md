---
title: Formatting
summary: Format Dream code from the editor or the command line.
icon: material/format-align-left
---

# Format Dream code

Use the same formatter in your editor and at the command line.

```sh
dream fmt src/
dream fmt app.dream tests/
```

Files are updated in place. A directory search is recursive, skips `target`, `.dream`, `node_modules`, `.git`, and `site`, does not follow symbolic links, and you can also pass a specific file or output directory.

## Check formatting in CI

```sh
dream fmt --check src/ tests/
```

The check leaves files unchanged and returns a nonzero exit status if a file needs
formatting, cannot be read, or cannot be safely formatted. Fix invalid tokens before
retrying a skipped file.

## Style

The formatter uses four spaces and a target width of 100 characters. Nonempty bodies use separate lines, long argument lists and expressions wrap, short calls stay compact, and attributes and documentation comments stay with the item they describe.

```dream
class Greeting {
    public static fun hello(name: string): string {
        return "Hello, " + name;
    }
}
```

One blank line separates declarations, and within a function the formatter keeps a single blank line between setup, validation, work, and cleanup. String contents and comments are kept, and long literals and comments can exceed the target width.

In the editor, use **Format Document**, or enable format on save for Dream files.
See [editor setup](editor.md) if the language tools are not available.
