# GenSourceMap

**Import:** `import system.codegen;`

## `GenSourceMap`

Associates generated output with the source locations it came from.

```dream
public class GenSourceMap
```

## `mappings`

Source locations associated with generated output.

```dream
public mappings: JsonValue
```

## `constructor`

Creates an empty builder.

```dream
public constructor()
```

## `add`

Maps a half-open artifact range to a snapshot node identity.

```dream
public fun add(line: int, column: int, end_line: int, end_column: int, target: string): void
```
