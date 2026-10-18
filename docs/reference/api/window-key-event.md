# KeyEvent

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `KeyEvent`

One key press or release, carried by `WindowEvent.KeyDown` and `WindowEvent.KeyUp`.

```dream
public struct KeyEvent
```

## `code`

The physical key, independent of the keyboard layout.

```dream
public code: KeyCode
```

## `key`

What the key prints on the active layout, such as `"q"` for `KeyCode.A` on AZERTY. Empty for keys that print nothing (arrows, function keys, modifiers) and when the platform cannot name the key. Use `WindowEvent.Text` for typed text.

```dream
public key: string
```

## `modifiers`

Modifier keys and lock states held when the event happened.

```dream
public modifiers: KeyModifiers
```

## `repeat`

True when the operating system repeated a held key. Repeats never change `KeyboardState.pressed`.

```dream
public repeat: bool
```

## `constructor`

Creates a key event. `simulate` uses events built this way to replay input.

```dream
public constructor(code: KeyCode, key: string, modifiers: KeyModifiers, repeat: bool)
```
