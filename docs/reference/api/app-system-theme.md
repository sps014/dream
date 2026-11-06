# System Theme

**Import:** `import system.app;`

Read the [usage guide](../../library/system/app.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `SystemTheme`

The color scheme the user chose for the operating system or browser, as reported by `Application.theme`.

```dream
public enum SystemTheme
```

## `Light`

Dark text on light backgrounds. This is also the value when the platform does not report a preference.

```dream
Light
```

## `Dark`

Light text on dark backgrounds.

```dream
Dark
```

## `HighContrast`

An accessibility mode with a small set of strongly contrasting colors, such as Windows contrast themes or the browser `forced-colors` mode.

```dream
HighContrast
```
