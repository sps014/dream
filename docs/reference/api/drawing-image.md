# Image

**Import:** `import system.drawing;`

## `Image`

An immutable decoded image. Canvas recordings retain it until they are released or cleared.

```dream
public class Image: DrawingResource
```

## `decode`

Decodes PNG or JPEG bytes. Input is copied; malformed data, excessive dimensions or more than 64 MiB of encoded data return an error.

```dream
public static async fun decode(bytes: byte[]): Result<Image, DrawingError>
```

## `from_rgba`

Copies tightly packed straight-alpha sRGB RGBA8 pixels, top row first. Requires width * height * 4 bytes and the surface dimension limits.

```dream
public static async fun from_rgba(
    width: int,
    height: int,
    bytes: byte[]
): Result<Image, DrawingError>
```

## `width`

Width of the decoded image in pixels.

```dream
public get width(): int
```

## `height`

Height of the decoded image in pixels.

```dream
public get height(): int
```
