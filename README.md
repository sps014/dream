# Dream

Dream is a programming language for building command-line tools, desktop apps, web services, and programs that run in a browser. It checks your code before running it and manages memory for you.

Start with a small program, then add the features you need. The included library gives you collections, files, network requests, JSON, dates, testing, graphics, and more.

[Documentation](https://sps014.github.io/dream/) · [Quickstart](docs/learn/quickstart.md) · [Language tour](docs/learn/tour.md) · [Examples](docs/cookbook/index.md)

## Install and run

On Windows, open PowerShell:

```powershell
irm https://sps014.github.io/dream/install.ps1 | iex
```

On macOS or Linux, open a terminal:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sps014.github.io/dream/install.sh | sh
```

Open a new terminal after installing, then create a project:

```sh
dreamer init hello
cd hello
dreamer run
```

The project contains `dream.toml`, your project settings, and `src/main.dream`, your first program:

```dream
import system;

fun main() {
    System.println("Hello, world!");
}
```

Output:

```text
Hello, world!
```

`main` is where your program starts. `import system;` makes console helpers available. `System.println` writes a line of text.

You can also save this program as `hello.dream` and run it with `dream run hello.dream`. See [Installation and your first run](docs/learn/quickstart.md) for setup details.

## Write useful programs

Variables hold values. Functions let you name a task and reuse it. Lists hold a changing number of items:

```dream
import system;
import system.collections;

fun total(borrow prices: List<int>): int {
    let sum = 0;
    for (let price in prices) {
        sum = sum + price;
    }
    return sum;
}

fun main() {
    let prices: List<int> = [12, 8, 5];
    prices.push(10);
    System.println(total(prices));
}
```

This prints `35`. `let` creates a variable you can change; use `const` when it must keep its value. The `for` loop visits each price. The `borrow` parameter lets the function read the list without taking ownership of it.

When an operation can fail, its result makes that explicit. You can handle both outcomes:

```dream
import system;

fun main() {
    switch (int.parse("42")) {
        Ok(value) => System.println(value),
        Err(error) => System.println(error.message()),
    }
}
```

This prints `42`. [Option and Result](docs/reference/stdlib/option-result.md) explains missing values and failures.

## What can you build?

| Task | Start here |
| --- | --- |
| Learn the basics | [Language tour](docs/learn/tour.md) |
| Build a small complete project | [Shopping total](docs/learn/first-project.md) |
| Store and search data | [Collections](docs/reference/stdlib/collections.md) |
| Read and write files | [Files](docs/reference/stdlib/file.md) |
| Call a web service | [HTTP](docs/reference/stdlib/http.md) |
| Build a web service | [Web APIs](docs/reference/stdlib/webapi.md) |
| Work with structured data | [JSON](docs/reference/stdlib/json.md) |
| Create a desktop window | [WebView](docs/reference/stdlib/webview.md) |
| Draw graphics or run GPU calculations | [GPU](docs/reference/stdlib/gpu.md) |
| Check your program | [Testing](docs/reference/tooling/testing.md) |
| Find an exact API signature | [API catalog](docs/reference/api/index.md) |

Desktop, browser, and Node programs do not all have the same permissions or services. Each feature guide explains its availability. Start with [Choosing where your program runs](docs/learn/environments.md).

## Everyday commands

Run these from your project folder:

| Command | Purpose |
| --- | --- |
| `dreamer run` | Build and start your app |
| `dreamer build --release` | Build an optimized version |
| `dreamer test` | Run the tests in `tests/` |
| `dreamer add <package>` | Install a reusable package |
| `dreamer pack` | Prepare a desktop app for sharing |
| `dreamer toolchain doctor` | Check required build tools |

The [Dreamer overview](docs/reference/tooling/dreamer.md) links to focused guides. [Packaging](docs/reference/tooling/pack/index.md) explains sharing an app; [publishing](docs/reference/tooling/publishing.md) explains sharing its source.

## Editor support and help

Dream includes editor support for code completion, error messages, formatting, and navigation. Follow [Editor setup](docs/reference/tooling/editor.md). If a command or build fails, use [Troubleshooting](docs/learn/troubleshooting.md).

Report a reproducible problem through [GitHub Issues](https://github.com/sps014/dream/issues). Include a small example, the command you ran, and the error text.

## Contribute

To work on Dream itself, read the [contributor handbook](docs/internals/README.md). It covers setup, the source layout, and the checks required before submitting changes. The [documentation maintenance guide](docs/internals/documentation.md) explains checking API coverage and examples.

## License

Dream is licensed under the MIT license.
