# Monorepo sample

Minimal Dream `[workspace]` layout:

```text
sample/monorepo/
  dream.toml                 # [workspace] members = [...]
  packages/greeter/          # lib member
  apps/cli/                  # bin member (path-depends on greeter)
```

```bash
cd sample/monorepo
dreamer install
dreamer run -p cli
# or:
cd apps/cli && dreamer run
```

After install, `dream.lock` and `dream_packages/` live at the workspace root; each member gets a
`dream_packages` symlink so the LSP and compiler resolve imports as usual.
