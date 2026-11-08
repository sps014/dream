---
title: Troubleshooting
summary: "Fix common installation, build, and project problems."
icon: material/lifebuoy
---

# Fix common setup and project problems

Start with the exact error message and the command that produced it. Make one change at a time, then run the same command again.

## The terminal cannot find Dream

Open a new terminal and run `dream --help` and `dreamer --help`. If neither works, check that the installer finished and that your command search path includes the Dream installation's `bin` folder.

If the terminal works but your editor does not, restart the editor and follow [Editor setup](../tooling/editor.md).

## A build tool is missing

Run `dreamer toolchain doctor`, install the missing component from [Toolchain setup](../tooling/toolchain.md), and repeat the build. For another platform, pass its target triple to `doctor`.

## An import cannot be found

Check the spelling and package name. Import a built-in package such as `system.collections` directly, and run `dreamer install` from the `dream.toml` folder for an external package.

External package names containing hyphens or dots use underscores in their import names. See [Dependencies](../tooling/dependencies.md) and [Imports](../language/rules/imports.md).

## Dreamer asks you to choose a target

A project listing more than one environment needs `--target native`, `--target web`, or `--target node`. Choose one declared in its project settings.

## A file or network operation fails

Read the returned `Err` value's `message()` and `code()`, and check the path or address, permissions, and environment support. Keep the error text when you need to know why an operation failed.

## Indexing stops the program

Array and list positions must be within their length, and `map[key]` requires a key that exists. When absence is normal, use `get` and handle its `Option` result ([Collections](../library/collections/index.md)).

## A browser page does not load the program

Use `dreamer run --target web` and open the served page. Check that the generated files exist, that the page uses their current paths, and that the browser console shows no loading or permission errors.

## Share a reproducible problem

Reduce your program to the smallest example that still fails. Include the program, command, Dream version, operating system, and complete error text, and leave out credentials and private data.
