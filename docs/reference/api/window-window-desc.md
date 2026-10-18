# WindowDesc

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `WindowDesc`

The settings a new window is created with.  Start from `WindowDesc.of` and chain the settings you want to change: `WindowDesc.of("Game", 1280, 720).resizable(false).fullscreen(true)`.

```dream
public class WindowDesc
```

## `of`

Describes a resizable, windowed window with `title` and a content area of `width` by `height` logical pixels. `Window.open` rejects non-positive sizes.

```dream
public static fun of(title: string, width: int, height: int): WindowDesc
```

## `title`

Title shown in the title bar, or as the page title in a browser.

```dream
public get title(): string
```

## `width`

Requested content width in logical pixels.

```dream
public get width(): int
```

## `height`

Requested content height in logical pixels.

```dream
public get height(): int
```

## `is_resizable`

Whether the user can resize the window. Defaults to `true`.

```dream
public get is_resizable(): bool
```

## `is_fullscreen`

Whether the window starts fullscreen on the primary monitor. Defaults to `false`.

```dream
public get is_fullscreen(): bool
```

## `wants_vsync`

Whether a GPU surface created for this window waits for the display's vertical refresh. Defaults to `true`. The window never draws by itself, so only GPU surfaces read this hint.

```dream
public get wants_vsync(): bool
```

## `resizable`

Sets whether the user can resize the window, and returns this description.

```dream
public fun resizable(value: bool): WindowDesc
```

## `fullscreen`

Sets whether the window starts fullscreen, and returns this description.

```dream
public fun fullscreen(value: bool): WindowDesc
```

## `vsync`

Sets the vertical-sync hint for GPU surfaces, and returns this description.

```dream
public fun vsync(value: bool): WindowDesc
```
