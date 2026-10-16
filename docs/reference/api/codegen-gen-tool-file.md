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

Text contents of the file.

```dream
public contents: string
```

## `binary`

Whether the message contains binary data.

```dream
public binary: bool
```

## `hash`

Hash of the refresh token; the raw refresh token is not stored.

```dream
public hash: string
```

## `constructor`

Reads a tool-output file description from its JSON representation.

```dream
public constructor(borrow v: JsonValue)
```
