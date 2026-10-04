# Clipboard

**Import:** `import system.desktop;`

Read the [usage guide](../stdlib/desktop.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class Clipboard`

System clipboard. Reads return empty when the format is missing or unreadable; failed writes are logged to stderr.

```dream
public class Clipboard
```

## `text`

```dream
public static get text(): string
```

## `text`

```dream
public static set text(value: string)
```

## `html`

Setting HTML also writes a plain-text copy for apps that can't paste HTML.

```dream
public static get html(): string
```

## `html`

```dream
public static set html(value: string)
```

## `image`

PNG bytes in and out.

```dream
public static get image(): byte[]
```

## `image`

```dream
public static set image(png: byte[])
```

## `files`

Paths of files copied in Finder / Explorer / the file manager.

```dream
public static get files(): List<string>
```

## `files`

```dream
public static set files(paths: List<string>)
```

## `formats`

Formats on the clipboard right now (MIME names or platform type identifiers).

```dream
public static get formats(): List<string>
```

## `set_data`

Raw bytes under a custom format name, e.g. `application/x-myapp-track`.

```dream
public static fun set_data(format: string, data: byte[]): void
```

## `data`

```dream
public static fun data(format: string): Option<byte[]>
```

## `has`

```dream
public static fun has(format: string): bool
```

## `clear`

```dream
public static fun clear(): void
```
