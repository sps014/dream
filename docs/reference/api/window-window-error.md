# WindowError

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `WindowError`

Why a window could not be created.

```dream
public class WindowError: Error
```

## `constructor`

Creates an error with a stable machine code and a readable message.

```dream
public constructor(code: string, message: string)
```

## `code`

The machine-readable reason. `ENODISPLAY` means no display is available, such as on a server, under Node, or when the platform window system failed to start. `ETHREAD` means the call came from a `Task` worker instead of the program's main thread, the only thread allowed to own windows. `ENOTFOUND` means `Window.canvas` found no `<canvas>` element with that id. `EINVAL` means the description has a non-positive size. `EWINDOW` means the platform refused to create the window.

```dream
public fun code(): string
```

## `message`

A human-readable description that includes the platform's own message when it gave one.

```dream
public fun message(): string
```
