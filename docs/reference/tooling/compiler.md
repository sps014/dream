# Build and run a single file

Use `dream` for a standalone `.dream` file. Use [Dreamer](dreamer.md) for projects with packages, project settings, and generated launchers.

```sh
dream run hello.dream
dream build hello.dream
dream run hello.dream -- first second
```

The first command builds and runs the file. The second only builds it. The third passes arguments to the program. With no file, Dream uses the entry file from the nearest `dream.toml`.

## Common commands

| Command | Purpose |
| --- | --- |
| `dream build file.dream` | Build without starting the program |
| `dream run file.dream` | Build and run on your computer |
| `dream test tests/` | Find and run test functions |
| `dream fmt src/` | Format source files in place |
| `dream fmt --check src/` | Check formatting without changing source |
| `dream debug-adapter` | Start the service used by an editor's debugger |
| `dream --help` | List current commands and options |
| `dream --version` | Show the installed version |

## Choose output and build settings

| Option | Purpose |
| --- | --- |
| `--release` | Build an optimized version |
| `-O0` through `-O4`, `-Os`, `-Oz` | Choose an optimization setting; overrides `--release` |
| `-O` | Use size optimization when no level follows |
| `-g`, `--debug-info` | Include debugging information |
| `-o path`, `--output path` | Choose an output path |
| `-v`, `--verbose` | Show more build progress |
| `--relocatable` | Put required native libraries beside the program for sharing |
| `--crate-type lib` or `bin` | Choose a library or runnable program |
| `--profile`, `--use-profile[=path]` | Record or use measurements to improve native builds |

Native output normally goes under `target/debug/` or `target/release/`. Browser output goes under `target/web/`. [Build and run](build-run.md) explains profile-guided builds. [Library output](../language/library-outputs.md) explains exported libraries.

## Browser and Node

Use `--web` or `--node` to build the program and its corresponding launcher support. These select WebAssembly output and imply `--runtime`. `--wasm` requests WebAssembly output without that generated support.

`--runtime-target native`, `web`, or `node` selects the availability rules checked for your code; it is separate from a platform target. Read [integration](../language/interop.md) before creating your own launcher.

## Another platform

`--target` takes a target triple identifying a platform and architecture. `--min-os` sets a minimum macOS version. Building an executable for another platform may require its SDK and target-specific libraries.

```sh
dream --object --target aarch64-unknown-linux-gnu -o hello.ll hello.dream
```

`--object` stops before linking. It writes an object and its description for later linking; it does not create a runnable executable. Inspect full build requirements with `dreamer toolchain doctor --target aarch64-unknown-linux-gnu`.

Foreign executables cannot be started with `run`, `test`, or the debug adapter on a different platform. Compiler-development output options belong in the [contributor handbook](../../internals/06-llvm-backend.md).
