---
title: Testing
summary: "Run `@test` functions in a test project."
icon: material/clipboard-check-outline
---

# Test a project

Tests check that a small part of your program behaves as intended. Mark test functions with `@test` and put them in a **test project**: a package of its own, or the `tests/` folder of the package it tests.

```dream
import system.testing;

@test
fun addition() {
    Assert.eq(2 + 3, 5);
}
```

Run the tests from your project folder:

```sh
dreamer test
dreamer test --filter addition
dreamer test --release
dreamer test --node
```

`--filter` keeps tests whose `<file>::<name>` label contains that text. Test runs also install development dependencies.

## `dreamer test` options

| Flag | Effect |
|---|---|
| `--filter <text>` | Run only tests whose `<file>::<name>` label contains the text |
| `--release` | Build the runner with the release profile |
| `-O <level>` / `--optimize <level>` | Release optimization level (`0`–`4`, `s`, `z`); requires `--release`. Debug test builds always run unoptimized |
| `--native` | Run on the native host instead of `[test].targets` |
| `--node` | Run under Node instead of `[test].targets`; with `--native`, run on both |
| `-p <name>` / `--package <name>` | Run one test project of a workspace |

## Test projects

A test project is a package with `type = "test"`. It has no entry and no `main`; every `.dream` file under it (except `target/`, `dream_packages/` and dot folders) is part of the suite.

```toml
[package]
name = "greeter_tests"
version = "0.1.0"
type = "test"
assets = ["fixtures"]           # optional files the tests read at run time

[dependencies]
greeter = { path = "../greeter" }

[test]
targets = ["native", "node"]    # hosts to run on (default: native)
```

`[test]` holds only `targets`, which accepts `native` and `node`. Test packages cannot be built, run, packed or published; use `dreamer test` or `dream test`.

Create one with `dreamer init test my_tests`, or create a program together with its test project with `dreamer init workspace myapp`.

A package that is not a test package can keep its tests in `tests/`. `dreamer test` inside it runs that folder as its test project.

## One runner per project

`dream test` compiles every `@test` into one runner, and skips compilation when nothing changed. `--filter` drops tests before compiling, so a filtered run builds only what it runs.

Tests may be synchronous or `async`, must return `void`, and run sequentially. An async test finishes before its pass report and before the next test begins.

Test functions can be file-private; the runner can call them. Labels use the file's path from the test project root, so `math/vectors.dream` is labeled `math.vectors::<name>`.

Each test prints one line:

```text
test math.vectors::adds ... ok
test math.vectors::normalizes ... FAILED
test result (native): FAILED. 1 passed; 1 failed; 0 not run
```

A failed assertion or a trap stops the runner. The test that was running is reported as failed and every later test as not run.

## Timeouts

There is no default timeout. Give a test a limit in milliseconds with `@timeout`:

```dream
@test
@timeout(2000)
fun finishes_quickly() {
// ...
}
```

When the limit passes, `dream test` stops the runner, reports the test as `TIMEOUT`, and reports the rest as not run.

## Leak checks

When a test leaves objects alive, its report includes a `LEAK <name> live=+N` hint. The run fails when `live` is not 0 at exit.

## Assets

Files listed in `[package].assets` are copied next to the runner, and the runner starts in that folder:

| Run | Runner folder |
|---|---|
| Native, debug | `target/test/debug` |
| Native, `--release` | `target/test/release` |
| Node | `target/test/node` |

Read them with relative paths:

```dream
@test
fun reads_fixture() {
    Assert.is_true(File.exists("fixtures/input.txt"));
}
```

## Hosts

`[test].targets` chooses the hosts. `--native` and `--node` override that choice for one run; pass both to run on both hosts, with the same leak report on Node.

## Workspaces

At a workspace root, `dreamer test` runs every test project (test packages and members with a `tests/` folder) in parallel, then prints one summary:

```text
── greeter_tests (tests/greeter)
...
2 test project(s): 2 passed, 0 failed
```

Use `-p name` to run one of them.

## The compiler command

`dream test [path]` runs one test project without installing dependencies. The path can be a test package, a folder of `.dream` files, or one file; with no path it uses the current test package, or `./tests`.

| Path | Tests that run |
|---|---|
| A `type = "test"` package folder | Every `.dream` file in it |
| Another package folder | The files in its `tests/` folder |
| A folder without `dream.toml` | Every `.dream` file in it |
| One `.dream` file | That file only |

`dream test` accepts `--filter`, `--native`, `--node` and `--release` with the same meaning as `dreamer test`.

See [Testing APIs](../library/system/testing.md) for assertion methods, [Project settings](project-settings.md) for `assets`, and [Workspaces](workspaces.md) for package selection. `sample/monorepo/` has two test projects, one of which reads assets.
