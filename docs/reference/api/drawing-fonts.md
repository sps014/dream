# Fonts

**Import:** `import system.drawing;`

## `Fonts`

Registers application fonts for subsequent text layout. The bundled Noto Sans requires no setup.

```dream
public static class Fonts
```

## `register`

Copies a TrueType, OpenType or collection file and returns its first family name. Existing TextLayouts retain their captured fonts.

```dream
public static async fun register(bytes: byte[]): Result<string, DrawingError>
```
