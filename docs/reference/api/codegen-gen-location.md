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

Creates a `GenLocation` from its parts. Generators normally receive these from `GenContext` rather than constructing them.

```dream
public constructor(file: string, line: int, column: int)
```

## `to_string`

The location as `file:line:column`.

```dream
public override fun to_string(): string
```
