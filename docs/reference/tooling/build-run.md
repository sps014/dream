# Build and run a project

Run your app while developing, or build it without starting it. Choose the environment your app needs.

### How `dreamer run` picks a host

| `package.targets` | No `--target` | With `--target X` |
|---|---|---|
| empty / omitted | **native** (`dream run`) | any of `native` / `web` / `node` (ad-hoc escape hatch) |
| exactly one entry | that host | must match that host |
| two or more | error — require `--target` | `X` must be one of the listed targets |

Per host:

- **native** — `dream run [--release] <entry> [args…]`.
- **node** — write `run.mjs` from `package.entry` if it is missing, compile with `--runtime --node`
  (refreshing `target/node/`), then `node run.mjs`.
- **web** — compile with `--runtime --web` (wasm in `target/web/`), then serve the project root on
  `http://127.0.0.1:8787/index.html` by default (colored log; Ctrl-C to stop). A later
  `dreamer run --target web` restarts that server on the same port. Override with `--port`.

Use `dreamer run --release` (optionally with `--target`) so release artifacts feed the same stable
alias paths the scaffolds already reference.


### Profile-guided native builds

Native builds can use profile-guided optimization in two steps. The same flags work on
`dream build` / `dream run` directly; they are rejected for wasm, `test`, and the debug adapter.

```bash
dreamer run --release --profile        # 1. instrumented binary; each run records into <bin>.pgo/
dreamer run --release --profile        #    (run as many representative workloads as you like)
dreamer run --release --use-profile    # 2. merge <bin>.pgo/*.profraw → <bin>.profdata, rebuild with it
dreamer build --release --use-profile=path/to/app.profdata   # or a .profraw / directory of them
```

`--profile` and `--use-profile` are mutually exclusive. Profiles accumulate across runs of the
same instrumented binary; rebuilding it (changed source or flags) clears the old ones. The
instrumented binary links through the pinned clang (zig's linker corrupts the profile counters),
and the merge uses the pinned `llvm-profdata`, so both come from `dreamer toolchain install llvm`.
