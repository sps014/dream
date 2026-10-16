---
title: Create a project
summary: Start a new project that runs.
icon: material/folder-plus-outline
---

# Create your first project

Start with a program that runs. Add packages and choose another environment after this first step works.

## Create and run

```sh
dreamer init hello
cd hello
dreamer run
```

`init` creates `dream.toml` for project settings and `src/main.dream` for your program. Open the source file and replace its contents with:

```dream
import system;

fun main() {
    System.println("Hello from my project!");
}
```

Run `dreamer run` again. It prints `Hello from my project!`. Use `dreamer build` when you want to build the program without starting it.

The [small project tutorial](../learn/first-project.md) adds data, a calculation, and a test. Read [project settings](project-settings.md) to change the entry file or application details.

## Choose a browser or Node project

```sh
dreamer init webapp --runtime web,node
cd webapp
dreamer run --target node
dreamer run --target web
```

Choose a target explicitly when your project supports several environments. Check [environment availability](../learn/environments.md) before adding file or process features.

## Other project shapes

```sh
dreamer init mylib --lib                    # a library (static by default)
dreamer init mylib --lib --output cdylib    # a library built as a shared library
dreamer init mytests --test                 # a test project
dreamer init myapp --workspace              # app/ + tests/ in one workspace
```

A test project holds `@test` functions and runs with `dreamer test`; see [Test a project](testing.md). A workspace keeps related packages together; see [Workspaces](workspaces.md).

## Add reusable code

Read [dependencies](dependencies.md) to add a registry package or a local package. `dreamer install` prepares dependencies, and `dreamer tree` shows what is installed. Use [publishing](../deploy/publishing.md) when your package is ready to share, or [packaging](../deploy/index.md) when you want to distribute an application.
