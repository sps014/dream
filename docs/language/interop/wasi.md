---
title: Standalone WASI
summary: Compile command components and grant filesystem and network access.
icon: material/console
status: experimental
targets: [wasip2, wasip3]
---

# Standalone WASI commands

Use a WASI target when a program should run in a component host without a JavaScript loader.
Dream supports command components for WASI Preview 2 and Preview 3.

Save this program as `hello.dream`:

```dream
import system;

fun main(): void {
    System.println("Hello from WASI");
}
```

Compile it and run the resulting component:

```sh
dream --target wasm32-wasip2 hello.dream
wasmtime run hello.wasm
```

The output is `Hello from WASI`. Compilation also writes readable `hello.wat` and
`hello.abi.json` files. The compiler uses a pinned WASI SDK.
Install it with `dreamer toolchain install wasi-sdk` before compiling.

For Preview 3, select `--target wasm32-wasip3`. With the pinned Wasmtime 49.0.4,
run the component with its Preview 3 features enabled:

```sh
wasmtime run -S p3=y -W component-model-async=y \
    -W component-model-more-async-builtins=y \
    -W component-model-async-stackful=y hello.wasm
```

## Files and network access

The component host controls access to files and networking. Grant a directory explicitly:

```sh
wasmtime run --dir ./data::. application.wasm
```

An ungranted path produces the usual `Result.Err` from Dream's filesystem APIs. For example:

```dream
import system;
import system.io;

async fun main(): void {
    let contents = File.read("input.txt").await;
    if contents.is_err() {
        System.println(contents.unwrap_err().message());
        return;
    }
    System.println(contents.unwrap());
}
```

Enable HTTP imports with `-S http=y`. Socket programs also need the host's network permissions;
Wasmtime grants them with `-S inherit-network=y`. Continue to handle `Result.Err` from network
operations because connection failures and host restrictions can still reject a request.

Command components provide stdio, files, clocks, secure randomness, TCP, UDP, and HTTP.
HTTP exposes individual responses; an explicit redirect limit reports an unsupported-operation
error. Unsupported host capabilities produce a compiler diagnostic. JavaScript interop, windowing,
GPU devices, process spawning, and CLI parsing are unavailable on these targets. Use native output
for CLI parsing, and [JavaScript interop](index.md) for browser and Node applications.

Components currently expose the command entry point. Exported component libraries require an
explicit WIT interface and are rejected by this target.
