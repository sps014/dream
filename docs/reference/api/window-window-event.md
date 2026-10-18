# WindowEvent

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `WindowEvent`

Something that happened to a window since the previous `Window.poll`.  `InputFrame.events` lists events in the order the operating system or browser reported them, followed by events queued with `Window.simulate`.

```dream
public enum WindowEvent
```

## `Resized`

The drawable area changed size. The size is in physical pixels, which is the size a GPU surface needs; it differs from `Window.size` on high-DPI displays.

```dream
Resized(size: WindowSize)
```

## `Moved`

The window moved; `x` and `y` are the new top-left corner in screen coordinates. Browsers never report this event.

```dream
Moved(x: int, y: int)
```

## `Focused`

The window gained keyboard focus.

```dream
Focused
```

## `Unfocused`

The window lost keyboard focus. Every held key and mouse button is reported released first, so input never sticks while the window is in the background.

```dream
Unfocused
```

## `CloseRequested`

The user asked to close the window, for example with its close button. `Window.is_open` is false after the poll that delivers this event.

```dream
CloseRequested
```

## `KeyDown`

A key went down, or the operating system repeated a held key.

```dream
KeyDown(key: KeyEvent)
```

## `KeyUp`

A key was released.

```dream
KeyUp(key: KeyEvent)
```

## `Text`

Text the user typed, after the keyboard layout, dead keys and input-method composition were applied. Consecutive characters arrive as one event.

```dream
Text(text: string)
```

## `MouseMove`

The cursor moved to `position`, in window coordinates.

```dream
MouseMove(position: WindowVector)
```

## `MouseDown`

A mouse button went down.

```dream
MouseDown(button: MouseButton)
```

## `MouseUp`

A mouse button was released.

```dream
MouseUp(button: MouseButton)
```

## `Scroll`

The wheel or touchpad scrolled. One wheel notch is `1.0`; positive `y` scrolls up and positive `x` scrolls right.

```dream
Scroll(delta: WindowVector)
```

## `GamepadConnected`

A gamepad was connected in `slot`. Gamepads already connected when the window opens are reported in the first poll.

```dream
GamepadConnected(slot: int)
```

## `GamepadDisconnected`

The gamepad in `slot` was disconnected.

```dream
GamepadDisconnected(slot: int)
```

## `FileDropped`

A file was dropped on the window. Native programs receive its full path; browsers only reveal the file name.

```dream
FileDropped(path: string)
```
