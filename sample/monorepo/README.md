# Monorepo sample

This example keeps a command-line app and a reusable library in one workspace. The app uses the library through a local dependency, while both packages share installed dependencies and one lockfile.

## About this example

Minimal Dream `[workspace]` layout:

```text
sample/monorepo/
  dream.toml                 # [workspace] members = [...]
  packages/greeter/          # lib member
  apps/cli/                  # bin member (path-depends on greeter)
  tests/greeter/             # test project for greeter (native + node)
  tests/fixtures/            # test project that reads its `data/` assets
```

```bash
cd sample/monorepo
dreamer install
dreamer run -p cli
# or:
cd apps/cli && dreamer run

dreamer test                 # every test project, in parallel, one summary
dreamer test -p fixture_tests
```

Each test project compiles all of its `@test` functions into one cached runner and runs it on
the hosts listed in its `[test] targets`.

After install, `dream.lock` and `dream_packages/` live at the workspace root; each member gets a
`dream_packages` symlink so the LSP and compiler resolve imports as usual.
