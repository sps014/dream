# GenLocation

**Import:** `import system.codegen;`

## `GenLocation`

A file, line, and column identifying a source location.

```dream
public class GenLocation
```

## `file`

Project-relative path (`src/models.dream`), or `<std>/...` for stdlib sources.

```dream
public file: string
```

## `line`

One-based line number in the source file.

```dream
public line: int
```

## `column`

One-based column number in the source file.

```dream
public column: int
```

## `constructor`

Creates the snapshot model from the supplied values.

```dream
public constructor(file: string, line: int, column: int)
```

## `to_string`

Returns the requested model information.

```dream
public override fun to_string(): string
```
