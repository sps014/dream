# Fix common setup and project problems

Start with the exact error message and the command that produced it. Make one change at a time, then run the same command again.

## The terminal cannot find Dream

Open a new terminal after installing. Run `dream --help` and `dreamer --help`. If neither works, check that the installer finished and that your command search path includes the Dream installation's `bin` folder.

If the terminal works but your editor does not, restart the editor and follow [Editor setup](../reference/tooling/editor.md).

## A build tool is missing

Run `dreamer toolchain doctor`. Install the missing component using [Toolchain setup](../reference/tooling/toolchain.md), then repeat the build. For another platform, pass its target triple to `doctor`.

## An import cannot be found

Check the spelling and package name. Import a built-in package such as `system.collections` directly. For an external package, run `dreamer install` from the folder containing `dream.toml`.

External package names containing hyphens or dots use underscores in their import names. See [Dependencies](../reference/tooling/dependencies.md) and [Imports](../reference/language/imports.md).

## Dreamer asks you to choose a target

A project listing more than one environment needs `--target native`, `--target web`, or `--target node`. Choose one declared in its project settings.

## A file or network operation fails

Read the returned `Err` value's `message()` and `code()`. Check the path or address, permissions, and environment support. Avoid replacing errors with empty strings when you need to know why an operation failed.

## Indexing stops the program

Array and list positions must be within their length. A map key must exist when you use `map[key]`. When absence is normal, use `get` and handle its `Option` result. See [Collections](../reference/stdlib/collections.md).

## A browser page does not load the program

Use `dreamer run --target web` and open the served page. Check that the generated files exist and that the page uses their current paths. Use the browser console to find loading or permission errors.

## Share a reproducible problem

Reduce your program to the smallest example that still fails. Include the program, command, Dream version, operating system, and complete error text in your issue. Keep credentials and private data out of the example.
