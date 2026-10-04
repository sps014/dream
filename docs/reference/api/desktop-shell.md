# Shell

**Import:** `import system.desktop;`

Read the [usage guide](../stdlib/desktop.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class Shell`

```dream
public class Shell
```

## `open`

Opens a URL in the default browser, or a file / folder with its default app.

```dream
public static fun open(target: string): Result<bool, DesktopError>
```
