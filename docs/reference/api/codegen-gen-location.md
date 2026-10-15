# GenLocation

**Import:** `import system.codegen;`

## `public class GenLocation`

```dream
public class GenLocation
```

## `public file: string`

Project-relative path (`src/models.dream`), or `<std>/...` for stdlib sources.

```dream
public file: string
```

## `public line: int`

```dream
public line: int
```

## `public column: int`

```dream
public column: int
```

## `public constructor(file: string, line: int, column: int)`

Creates the snapshot model from the supplied values.

```dream
public constructor(file: string, line: int, column: int)
```

## `to_string`

Returns the requested model information.

```dream
public override fun to_string(): string
```
