# Dream editor and project tools

This folder contains the tools used to work on Dream programs: the language server, editor extension, package manager, and documentation checks.

| Folder | Purpose |
| --- | --- |
| `dream-lsp/` | Completion, errors, navigation, and formatting for editors |
| `vscode/` | Dream extension for VS Code-compatible editors |
| `dreamer/` | Creating, building, testing, and packaging projects |
| `docs/` | Documentation highlighting, API coverage, and example checks |

## Use the editor tools

Install Dream and the Dream editor extension, then open your project folder. See [Editor setup](../docs/reference/tooling/editor.md) for executable discovery and settings.

The extension offers completion, hover documentation, parameter hints, definition navigation, reference search, rename within a document, outlines, quick fixes, run/debug actions, and formatting. It reads installed packages from your project; run `dreamer install` when adding dependencies.

## Build the extension

From the repository root:

```sh
cd tooling/vscode
npm install
npm run compile
npx @vscode/vsce package
```

Install the resulting `.vsix` through your editor's extension menu. Its filename includes the extension's current version. For development, open the extension project and press F5 to start an extension development window.

Build the server from the repository root with `cargo build -p dream-lsp`. Ensure the editor uses the rebuilt binary before testing changes; an older installed server will keep its older behavior.

## Check changes

```sh
cargo test -p dream-lsp
```

See [Documentation maintenance](../docs/internals/documentation.md) for API and example checks, and [Dreamer](../docs/reference/tooling/dreamer.md) for project commands.
