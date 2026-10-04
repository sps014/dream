# DesktopError

**Import:** `import system.desktop;`

Read the [usage guide](../stdlib/desktop.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class DesktopError : Error`

Failure from a native desktop call (clipboard write, opening a URL or path).

```dream
public class DesktopError : Error
```

## `constructor`

```dream
public constructor(code: string, message: string)
```

## `message`

```dream
public fun message(): string
```

## `code`

```dream
public fun code(): string
```

## `failed`

```dream
public static fun failed(message: string): DesktopError
```
