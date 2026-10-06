# Install Dream and run your first program

Follow these steps to install Dream, create a project, and run a small program. You can use the terminal on Windows, macOS, or Linux.

## Install

On Windows, open PowerShell:

```powershell
irm https://sps014.github.io/dream/install.ps1 | iex
```

On macOS or Linux:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sps014.github.io/dream/install.sh | sh
```

The installer puts Dream, Dreamer, and the editor support program in `~/.dream/bin`. It also installs the build tools your programs need.

Open a new terminal so it can find the installed commands, then check:

```sh
dream --help
dreamer --help
```

If a command cannot be found, use [Troubleshooting](troubleshooting.md).

## Create a project

```sh
dreamer init hello
cd hello
```

The project has a settings file, `dream.toml`, and a program in `src/main.dream`. Open the program and make it look like this:

```dream
import system;

fun main() {
    System.println("Hello, world!");
}
```

`import system;` makes console helpers available. `fun main()` is where execution begins. `System.println` writes the text followed by a new line.

## Run it

```sh
dreamer run
```

You should see:

```text
Hello, world!
```

Change the message and run the command again. Dream builds your current source each time.

## Run one file without a project

Save the same program as `hello.dream`, then use:

```sh
dream run hello.dream
```

Use `dream hello.dream` to build without running. For a project with dependencies, prefer `dreamer run` so its packages are installed too.

## Next steps

Continue with the [language tour](tour.md) and [your first small project](first-project.md). Choose the browser or Node later using [Environments](environments.md). Set up completion and error messages with [Editor support](../reference/tooling/editor.md).

## Installation options

Set `DREAM_VERSION` before running the installer to request a particular release. `DREAM_SKIP_LLVM=1` skips the build-tools download, and `DREAM_SKIP_CC=1` skips the native linker download; use these only when you already have suitable tools.

Linux release downloads require glibc 2.36 or later. Read [Toolchain setup](../reference/tooling/toolchain.md) to inspect or change your tools, and [the compiler command guide](../reference/tooling/compiler.md) for advanced output and target options.
