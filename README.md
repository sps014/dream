<div align="center">

# Dream

**Write expressive code. Ship native binaries and WebAssembly.**

A statically typed, ahead-of-time compiled language with automatic memory management,
specialized generics, and a standard library built for real applications.

**Optimized ARC** · **GPU shaders in Dream** · **Typed web APIs** · **Seamless C, C++, and JavaScript interop** · **Native + WebAssembly**

[Get started](#get-started) · [Documentation](https://sps014.github.io/dream/) · [Language tour](docs/learn/tour.md) · [Examples](docs/cookbook/index.md)

</div>

## Get started

On **macOS or Linux**:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sps014.github.io/dream/install.sh | sh
```

On **Windows**, open PowerShell:

```powershell
irm https://sps014.github.io/dream/install.ps1 | iex
```

Open a new terminal, then create and run a project:

```sh
dreamer init hello
cd hello
dreamer run
```

Your program lives in `src/main.dream`; project settings live in `dream.toml`. The generated starter prints `Hello, world!`. Replace it with the JSON example below to try a little more of the language.

Continue with the [quickstart](docs/learn/quickstart.md), [language tour](docs/learn/tour.md), or [first complete project](docs/learn/first-project.md).

## A first taste

```typescript
import system;
import system.json;

@json
class Developer {
    name: string;
    languages: string[];

    public constructor(name: string, languages: string[]) {
        this.name = name;
        this.languages = languages;
    }
}

fun main() {
    let dev = Developer("Ada", ["Dream", "TypeScript"]);
    System.println(Json.serialize(dev));
}
```

```json
{"name":"Ada","languages":["Dream","TypeScript"]}
```

Typed data, automatic cleanup, and JSON serialization generated at build time. Paste it into `src/main.dream` and run `dreamer run`.

Dream is **pre-1.0 and actively evolving**. Expect breaking changes as the language develops.

## Why choose Dream?

Dream is for people who want familiar application code, ahead-of-time compilation, and control over where their program runs. If you know TypeScript, Rust, or C#, much of the syntax will feel familiar. The interesting part is how the pieces fit together.

### Compile to the machine. Bring the same language to the web.

Build native executables for command-line tools and services, or compile to WebAssembly for the browser and Node. Share your core logic across targets, then use each environment's APIs where you need them. Code goes through LLVM's optimizer either way, and builds are deterministic: the same source produces byte-identical output. See [environments](docs/learn/environments.md).

### Write GPU shaders in Dream.

Import `system.gpu` and write compute, vertex, and fragment shaders as Dream functions. Typed buffers and generated shader calls connect your application to the GPU. Shaders compile ahead of time, so no shader source or shader compiler ships with your program.

Start with WebGPU for native and browser programs, or choose Metal, Vulkan, Direct3D, or CUDA for platform-specific work. Rendering, ray tracing, automatic differentiation, and tensor hardware operations are available where the selected backend and GPU support them; CUDA is compute-only, and ray tracing requires a native backend.

Install the shader compiler before building a program with shaders:

```sh
dreamer toolchain install slang
```

Follow the [GPU setup and first compute shader](docs/library/gpu/index.md), draw with the [rendering guide](docs/library/gpu/rendering.md), or try the [interactive spotlight](docs/cookbook/gpu-spotlight.md). Explore [ray tracing](docs/library/gpu/ray-tracing.md) and [machine learning](docs/library/gpu/machine-learning.md) for advanced workloads. The setup guide lists additional tools required by specific targets; GPU failures are returned as `Result` values with `GpuError` details.

### Automatic memory that mostly compiles away.

Dream manages heap values with automatic reference counting (ARC). Objects are freed the moment their last owner lets go, with no garbage collector and no tracing pauses. Most of the counting is removed at compile time:

- **Moves on last use.** Passing or storing a value for the last time hands it over instead of copying a reference. There is no `move` keyword to sprinkle through your code.
- **Borrow inference.** Release builds work out which parameters only read their arguments and pass those without touching the count. Write `borrow` when you want the contract spelled out.
- **Retain/release elision.** Pairs that cancel out are removed across branches, loops, and inlined calls.
- **Retain-free traversal.** Walking a linked structure (`node = node.next`) or reading fields in a loop does no counting when the compiler can prove the source stays alive.
- **Cheap counts where it matters.** Counts are plain, non-atomic integers unless a type is declared `shared` for tasks. Interned strings never change their count, and objects that never escape a function can be split into local values with no heap allocation.
- **Zero-cost generics.** Generic code is specialized for each concrete type, so there is no runtime type dispatch.

Classes share identity; structs copy as values and live inline. Ordinary code cannot read freed memory, out-of-bounds indexing stops with a clear error instead of reading past the end, and classes that can form reference cycles must be marked `@allow_cycle`, which makes leaks visible. Read about [ownership](docs/language/memory/ownership.md), [memory management](docs/language/memory/index.md), and [reference cycles](docs/language/memory/cycles.md).

### Call C, C++, and JavaScript directly.

Declare a foreign function with the right attribute and call it like any Dream function. The compiler generates the glue and converts values at the boundary.


| Interop | Runs on | What you get |
| --- | --- | --- |
| [C](docs/language/interop/c.md) | Native, browser, Node | `@c` externs. Package C sources are built with the pinned clang for native and WebAssembly; desktop builds can also link system libraries. [Native callbacks](docs/language/interop/native-callbacks.md) let C call back into Dream. |
| [C++](docs/language/interop/cpp.md) | Same as C | `@cpp` maps classes, methods, constructors, and templates. `std::optional`, exceptions, and `std::function` map to `Option`, `Result`, and Dream closures. |
| [JavaScript](docs/language/interop/index.md) | Browser, Node | `extern fun` for known signatures, the dynamic [`js` type](docs/language/interop/js-type.md) for DOM nodes and other live values, [callbacks](docs/language/interop/callbacks.md) in both directions, and `extern async fun` for Promises. An optional tree-shaken runtime ships only the host code your program uses. |

### Serve typed web APIs.

`system.webapi` builds native HTTP services. Routes are typed static methods, and the router is generated at compile time, so a handler cannot receive an argument of the wrong type.

```typescript
import system;
import system.json;
import system.webapi;

@json
class Greeting {
    public message: string;

    public constructor(message: string) {
        this.message = message;
    }
}

@router("")
class Routes {
    @get("/hello/{name}")
    public static fun hello(@path name: string): Greeting {
        return Greeting("Hello, " + name);
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

Run it with `dream run app.dream` and open `http://127.0.0.1:8000/hello/Dream`. Interactive Swagger documentation is served at `/docs`.

The framework also includes [validated request models](docs/web-server/validation.md), [dependency injection](docs/web-server/dependencies.md), [middleware and CORS](docs/web-server/middleware.md), [password and token authentication](docs/web-server/authentication.md), [OAuth2/OIDC login](docs/web-server/oidc.md), [scopes](docs/web-server/authorization.md), [uploads](docs/web-server/uploads.md), [server-sent events](docs/web-server/streaming.md), [WebSockets](docs/web-server/websockets.md), and [TLS](docs/deploy/web-server.md). Servers run on native targets. Start with the [web server guide](docs/web-server/index.md).

### Keep abstractions in your source, specialize them at build time.

Write reusable collections and algorithms with generics. Dream specializes generic code for the concrete types you use, avoiding runtime type dispatch for those generic operations.

Use classes and interfaces for object-oriented designs, structs for value data, and enums and discriminated unions to model alternatives. `Option` and `Result` make missing values and recoverable failures part of a function's type. Explore the [type system](docs/language/types/index.md) and [error handling](docs/library/core/option-result.md).

### Wait concurrently. Compute in parallel. Own the lifetime of both.

Use `async`/`await` for work that waits and worker tasks for CPU parallelism. Structured task scopes join their children on success, failure, cancellation, and early error return, so concurrent work has a clear lifetime.

Start with [structured concurrency](docs/language/concurrency/structured.md), then use [tasks](docs/language/concurrency/tasks.md) and pools when you need parallel execution.

### Extend the language using the language.

**Dream source generators are ordinary Dream functions.** They can inspect declarations, generate methods, and turn custom syntax blocks into checked Dream code during a build.

The `@json` in the opening example uses this same mechanism. Your own packages can ship derives and small domain-specific notations, too. See [source generators](docs/language/metaprogramming/generators.md) for a complete example.

## Build something useful

The standard library covers collections, files, JSON, HTTP, regex, dates, crypto, processes, logging, and testing. Dream also includes a web API framework with typed routes, validation, and generated OpenAPI documentation.

- **CLI tools and automation:** work with [files](docs/library/io/file.md), [processes](docs/library/system/process.md), and [command-line arguments](docs/library/system/cli.md).
- **HTTP services:** build your [first web API](docs/web-server/first-application.md), then add validation, authentication, and streaming.
- **Browser and Node modules:** compile to WebAssembly and connect to [JavaScript APIs](docs/language/interop/js-type.md).
- **GPU applications:** run [compute shaders](docs/library/gpu/index.md), build [renderers](docs/library/gpu/rendering.md), or explore [GPU machine learning](docs/library/gpu/machine-learning.md).
- **Reusable libraries:** combine [collections](docs/library/collections/index.md), generics, and source generators, then [publish a package](docs/deploy/publishing.md).

Each feature guide explains which environments it supports.

## A toolchain for everyday work

Dreamer handles projects, dependencies, builds, tests, and packaging:

```sh
dreamer run                # Build and run your app
dreamer build --release    # Build an optimized version
dreamer test               # Run your project's tests
dreamer add <package>      # Add a dependency
dreamer pack               # Package a CLI app for sharing
dreamer toolchain doctor   # Check required build tools
```

Dream includes editor completion, diagnostics, formatting, and navigation. Follow [editor setup](docs/tooling/editor.md), browse the [Dreamer guide](docs/tooling/index.md), or use [troubleshooting](docs/learn/troubleshooting.md) if you get stuck.

## Help shape Dream

Found a bug or have a language idea? Open a [GitHub issue](https://github.com/sps014/dream/issues). For bugs, include a small program, the command you ran, and the error text.

To work on the compiler, runtime, or tools, start with the [contributor handbook](docs/internals/README.md). For documentation changes, read the [maintenance guide](docs/internals/documentation.md).

## License

[MIT](LICENSE)
