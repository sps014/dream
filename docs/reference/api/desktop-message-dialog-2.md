# MessageLevel, MessageButtons, MessageResult, MessageDialog, Dialog

**Import:** `import system.desktop;`

Read the [usage guide](../stdlib/desktop.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](desktop-message-dialog.md)

## `parent`

`WebView.window_id` of the owning window; `-1` for none.

```dream
public get parent(): int
```

## `parent`

```dream
public set parent(value: int)
```

## `show`

```dream
public async fun show(): MessageResult
```

## `class Dialog`

One-line message boxes.

```dream
public class Dialog
```

## `alert`

```dream
public static async fun alert(text: string): void
```

## `confirm`

True when the user picks Yes.

```dream
public static async fun confirm(text: string): bool
```
