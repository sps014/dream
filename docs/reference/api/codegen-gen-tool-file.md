# GenToolFile

**Import:** `import system.codegen;`

## `GenToolFile`

A file produced by a tool requested during generation.

```dream
public class GenToolFile
```

## `path`

Request or resource path.

```dream
public path: string
```

## `contents`

File contents: text, or base64 when `binary` is true. Empty for an opaque output.

```dream
public contents: string
```

## `binary`

Whether the file is not valid UTF-8 text, so `contents` holds base64.

```dream
public binary: bool
```

## `hash`

BLAKE3 hash of the file bytes, as lowercase hex.

```dream
public hash: string
```

## `size`

Size of the file in bytes.

```dream
public size: int
```

## `opaque`

Whether the file was requested as an opaque output. Its bytes are not in the snapshot; embed or publish it by referring to it as `tool:<request id>/<path>`.

```dream
public opaque: bool
```

## `constructor`

Reads a tool-output file description from its JSON representation.

```dream
public constructor(borrow v: JsonValue)
```
