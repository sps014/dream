---
title: Conditional compilation
summary: "Keep code for one platform or build profile with `@cfg(Target.X)`."
icon: material/call-split
status: stable
targets: [native, web, node]
---

# Conditional compilation

Use `@cfg` when part of a program applies to only some builds: one operating system, WebAssembly,
or debug builds. The compiler keeps the marked code when the condition holds for the build and
removes it completely when it does not.

!!! glance "At a glance"

    ```dream
    @cfg(Target.Windows)
    fun config_dir(): string {
        return "C:\\ProgramData\\app";
    }

    @cfg(!Target.Windows)
    fun config_dir(): string {
        return "/etc/app";
    }
    ```

    - Write `@cfg(condition)` as the first attribute of a declaration, member, or statement.
    - Combine targets with `!`, `&&`, `||`, and parentheses.
    - Removed code is never type-checked, so it may use APIs that exist only on other targets.

## Choose a declaration per target

Put `@cfg` on each version of a function, class, struct, enum, interface, `extend` block, global,
type alias, or `import`. Exactly one version should remain for every build you support:

```dream
import system;

@cfg(Target.Native)
fun runtime_name(): string {
    return "native";
}

@cfg(Target.Wasm)
fun runtime_name(): string {
    return "WebAssembly";
}

fun main(): void {
    System.println("Running on " + runtime_name());
}
```

A native build prints `Running on native`, and a `--wasm` build prints `Running on WebAssembly`. If two versions remain, the name is a duplicate, and if none remains, calls report it as undefined.

## Gate members and enum choices

`@cfg` also works on fields, methods, constructors, interface methods, and enum choices:

```dream
class Settings {
    public name: string;

    @cfg(Target.Debug)
    public trace: bool;

    public constructor(name: string) {
        this.name = name;

        @cfg(Target.Debug)
        this.trace = true;
    }
}

enum Backend {
    Cpu,

    @cfg(Target.Native)
    Threads,

    @cfg(Target.Wasm)
    Worker,
}
```

In a release build, `Settings` has no `trace` field, so every use of `trace` needs the same
condition. A `switch` over `Backend` lists only the choices that remain for the build.

## Gate statements

A statement inside a function body can carry `@cfg`. To control several statements at once, put
them in braces after the attribute:

```dream
fun start(): void {
    @cfg(Target.Debug)
    System.println("starting in debug mode");

    @cfg(Target.Native) {
        let threads = 4;
        System.println("worker threads: " + threads.to_string());
    }
}
```

The braces group statements for `@cfg` only; they do not start a new scope. A `let` inside them
stays visible after the closing brace, as if the condition were written on each statement.

## Conditions

A condition names targets as `Target.<name>`. The names a build defines:

| Name | Defined when |
|---|---|
| `Target.Debug` | building without `--release` |
| `Target.Release` | building with `--release` |
| `Target.Native` | building a native binary |
| `Target.Wasm` | building a WebAssembly module |
| `Target.Js` | building WebAssembly for a JavaScript host: a browser, Node.js, or both |
| `Target.Browser` | building WebAssembly for a browser (the default for `--wasm`, and `--runtime --web`) |
| `Target.Node` | building WebAssembly for Node.js (`--runtime --node`) |
| `Target.Wasi` | building WebAssembly for a WASI host |
| `Target.Windows` | building a native binary for Windows |
| `Target.Macos` | building a native binary for macOS |
| `Target.Linux` | building a native binary for Linux |
| `Target.Android` | building a native binary for Android |
| `Target.Ios` | building a native binary for iOS |

Every build defines exactly one of `Debug` and `Release`, and exactly one of `Native` and `Wasm`. A native build also defines its operating system. A WebAssembly build defines either `Wasi` or `Js`; a `Js` build also defines `Browser` or `Node` for its host. One module built for both hosts (`--runtime --web --node`) defines neither, so it only sees code that works on both.

Use these names to keep an API to the hosts that can run it. A program that calls a function the current build leaves out gets an error that the function does not exist:

```dream
@cfg(Target.Native || Target.Node)
fun exit_with(code: int): void {
    System.exit(code);
}
```

Pass `--target` with a target triple to build for another operating system.

Combine names with `!` (not), `&&` (and), and `||` (or). `!` binds tightest, then `&&`, then `||`;
use parentheses to group differently:

```dream
@cfg(Target.Linux || Target.Macos)
fun uses_posix_paths(): bool {
    return true;
}

@cfg(Target.Native && !(Target.Windows || Target.Android || Target.Ios))
fun can_fork(): bool {
    return true;
}
```

The set of names is fixed. A misspelled name such as `Target.Win` is a compile error listing the
valid names, and `Target` cannot be used outside `@cfg(...)`.

## Rules

- `@cfg` must be the first attribute: `@cfg(Target.Debug) @test fun probe()`. Attributes after it
  apply only when the item remains.
- An item takes at most one `@cfg`. Combine conditions with `&&` or `||` instead of stacking them.
- Removed code must still parse, but it is not type-checked. Write code for other targets as
  ordinary Dream that compiles on those targets.
- Editors show removed code dimmed for the current host and debug profile.
