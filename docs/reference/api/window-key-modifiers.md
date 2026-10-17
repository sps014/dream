# KeyModifiers

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `KeyModifiers`

Modifier keys and lock states held while a key event happened.  The choices are bit flags. Test one with `&`, for example `(key.modifiers & KeyModifiers.Shift) != KeyModifiers.None`.

```dream
public enum KeyModifiers
```

## `None`

No modifier was held.

```dream
None = 0
```

## `Shift`

Either Shift key was held.

```dream
Shift = 1
```

## `Control`

Either Control key was held.

```dream
Control = 2
```

## `Alt`

Either Alt (Option) key was held.

```dream
Alt = 4
```

## `Meta`

Either Meta key (Command on macOS, Windows key on Windows) was held.

```dream
Meta = 8
```

## `CapsLock`

Caps Lock was on.

```dream
CapsLock = 16
```

## `NumLock`

Num Lock was on.

```dream
NumLock = 32
```
