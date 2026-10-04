# MessageLevel, MessageButtons, MessageResult, MessageDialog, Dialog

**Import:** `import system.desktop;`

Read the [usage guide](../stdlib/desktop.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](desktop-message-dialog.md)

## `enum MessageLevel`

```dream
public enum MessageLevel
```

## `Info`

```dream
Info
```

## `Warning`

```dream
Warning
```

## `Error`

```dream
Error
```

## `enum MessageButtons`

```dream
public enum MessageButtons
```

## `Ok`

```dream
Ok
```

## `OkCancel`

```dream
OkCancel
```

## `YesNo`

```dream
YesNo
```

## `YesNoCancel`

```dream
YesNoCancel
```

## `enum MessageResult`

```dream
public enum MessageResult
```

## `Ok`

```dream
Ok
```

## `Cancel`

```dream
Cancel
```

## `Yes`

```dream
Yes
```

## `No`

```dream
No
```

## `class MessageDialog`

Native alert / confirm box. Closing the box without choosing reads as `Cancel`.

```dream
public class MessageDialog
```

## `constructor`

```dream
public constructor()
```

## `title`

```dream
public get title(): string
```

## `title`

```dream
public set title(value: string)
```

## `text`

```dream
public get text(): string
```

## `text`

```dream
public set text(value: string)
```

## `level`

```dream
public get level(): MessageLevel
```

## `level`

```dream
public set level(value: MessageLevel)
```

## `buttons`

```dream
public get buttons(): MessageButtons
```

## `buttons`

```dream
public set buttons(value: MessageButtons)
```
