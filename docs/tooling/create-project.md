---
title: Create a project
summary: Choose a project template, then build and run it.
icon: material/folder-plus-outline
---

# Create a project

Use `dreamer init [template] [name]` to create a project. With no arguments, it creates
a console program in the current directory. Supplying a name creates a new directory:

```sh
dreamer init console hello
cd hello
dreamer run
```

Edit `src/main.dream` and run `dreamer run` again to rebuild and execute your changes.
For example:

```dream
import system;

fun main() {
    System.println("Hello from my project!");
}
```

This prints `Hello from my project!`. Use `dreamer build` to compile without starting.
The [small project tutorial](../learn/first-project.md) adds data, a calculation, and a test.

## Start a canvas app

Choose the platforms when creating the project:

```sh
dreamer init desktop counter
```

`desktop` creates a GPU-backed canvas counter for the current desktop OS. Its drawing,
mouse input, resize handling, and scale handling live in `src/main.dream`. The drawing
renderer includes Noto Sans, so the starter does not depend on installed fonts.

```sh
cd counter
dreamer build
dreamer run
```

The window contains an Add one button. Clicking and releasing inside it increments the
counter. Rendering failures, including an unavailable GPU, return a readable error from
`main`. Minimized or hidden windows pause drawing until visible again.

Use `dreamer init mobile phone` for an Android/iOS project, or `dreamer init app counter`
for a project declaring desktop, Android, and iOS. Without a name, `dreamer init mobile`
and `dreamer init app` initialize the current directory.

All three templates generate the same canvas source. `app` defaults to desktop for
`dreamer build` and `dreamer run`. `mobile` requires an explicit target because it declares
two platforms. `dreamer build --target android` produces a debug APK and
`dreamer build --target ios` produces a simulator app; `dreamer run` installs and launches
one on a running device. See [mobile tooling](../deploy/mobile.md) for the SDK prerequisites.

The starter handles touch events as well as mouse input, and offsets its content by
`Window.safe_area`.

## Browser and Node projects

```sh
dreamer init web browser-app
cd browser-app
dreamer run
```

The browser template includes `index.html`, which loads the build from `target/web/`.
`dreamer init node server` creates a Node project with `run.mjs` and output under
`target/node/`. Check [environment availability](../learn/environments.md) before adding
file or process features. To support both environments, edit `package.targets` in
`dream.toml` to `["web", "node"]` and select one with `dreamer run --target web` or
`--target node`; provide an HTML launcher if adding the web target to a Node project.

## Libraries, tests, and workspaces

```sh
dreamer init lib mylib
dreamer init lib shared --output cdylib
dreamer init test mytests
dreamer init workspace myproject
```

`lib` creates `src/<name>.dream` and a library manifest. The default output is `staticlib`;
`--output cdylib` selects a shared library. `--output` is only accepted for `lib`.

`test` creates an example `@test` function and a `fixtures/` asset directory. Run it with
`dreamer test`; see [testing](testing.md). `workspace` creates an `app/` console program
and a `tests/` project that imports it; see [workspaces](workspaces.md).

## Directories and existing files

Use `--dir <path>` to choose a destination independently of the package name:

```sh
dreamer init app counter --dir apps/counter
```

A named destination must be empty or missing. Initialization refuses to replace an
existing `dream.toml`, and preserves an existing entry source when initializing in place.
Builds do not regenerate the starter source, so edits remain yours.

Every template writes `.gitignore` entries for `dream_packages/`, `target/`, `.dream/`,
and `generated/`. Library source filenames replace hyphens and dots in the package name
with underscores. See [project settings](project-settings.md) for application identity,
deployment minimums, and assets.
