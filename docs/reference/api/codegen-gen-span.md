# GenSpan

**Import:** `import system.codegen;`

## `GenSpan`

A source range described by its starting location and length.

```dream
public class GenSpan
```

## `file`

Source file path.

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

## `length`

Number of elements in this range.

```dream
public length: int
```

## `constructor`

Creates this model from snapshot JSON.

```dream
public constructor(borrow v: JsonValue)
```
