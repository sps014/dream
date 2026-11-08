---
title: Create a project
summary: Choose a project template, then build and run it.
icon: material/folder-plus-outline
---

# Create a project

Use `dreamer init [template] [name]` to create a project. With no arguments it creates a console program in the current directory, and a name creates a new directory:

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

This prints `Hello from my project!`. Use `dreamer build` to compile without starting, or follow the [small project tutorial](../learn/first-project.md) to add data, a calculation, and a test.

## Start a canvas app

Choose the platforms when creating the project:

```sh
dreamer init desktop counter
```

`desktop` creates a GPU-backed canvas counter for the current desktop OS. Drawing, mouse input, resize, and scale handling live in `src/main.dream`, and the starter includes Noto Sans so it does not need installed fonts.

```sh
cd counter
dreamer build
dreamer run
```

The window contains an Add one button, and clicking and releasing inside it increments the counter. A rendering failure, including an unavailable GPU, returns an error from `main`, and a minimized or hidden window pauses drawing until it is visible again.

Use `dreamer init mobile phone` for an Android/iOS project, or `dreamer init app counter`
for a project declaring desktop, Android, and iOS. Without a name, `dreamer init mobile`
and `dreamer init app` initialize the current directory.

All three templates generate the same canvas source. `app` defaults to desktop for `dreamer build` and `dreamer run`, while `mobile` needs an explicit target because it declares two platforms.

`dreamer build --target android` produces a debug APK, and `dreamer build --target ios` produces a simulator app. `dreamer run` installs and launches that build on a running device ([mobile tooling](../deploy/mobile.md)).

The starter handles touch events as well as mouse input, and offsets its content by
`Window.safe_area`.

## Browser and Node projects

```sh
dreamer init web browser-app
cd browser-app
dreamer run
```

The browser template includes `index.html`, which loads the build from `target/web/`. `dreamer init node server` creates a Node project with `run.mjs` and output under `target/node/`.

Check [environment availability](../learn/environments.md) before adding file or process features. To support both, set `package.targets` to `["web", "node"]` and select one with `dreamer run --target web` or `--target node`, and add an HTML launcher if you add the web target to a Node project.

## Libraries, tests, and workspaces

```sh
dreamer init lib mylib
dreamer init lib shared --output cdylib
dreamer init test mytests
dreamer init workspace myproject
```

`lib` creates `src/<name>.dream` and a library manifest. The default output is `staticlib`, `--output cdylib` selects a shared library, and `--output` is accepted only for `lib`.

`test` creates an example `@test` function and a `fixtures/` directory; run it with `dreamer test` ([testing](testing.md)). `workspace` creates an `app/` console program and a `tests/` project that imports it ([workspaces](workspaces.md)).

## Directories and existing files

Use `--dir <path>` to choose a destination independently of the package name:

```sh
dreamer init app counter --dir apps/counter
```

A named destination must be empty or missing. Initialization will not replace an existing `dream.toml`, it keeps an existing entry source when you initialize in place, and later builds do not regenerate the starter.

Every template writes `.gitignore` entries for `dream_packages/`, `target/`, `.dream/`, and `generated/`. Library source filenames replace hyphens and dots in the package name with underscores ([project settings](project-settings.md)).
