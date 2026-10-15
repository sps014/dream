# GenSourceMap

**Import:** `import system.codegen;`

## `public class GenSourceMap`

```dream
public class GenSourceMap
```

## `public mappings: JsonValue`

```dream
public mappings: JsonValue
```

## `public constructor()`

Creates an empty builder.

```dream
public constructor()
```

## `add`

Maps a half-open artifact range to a snapshot node identity.

```dream
public fun add(line: int, column: int, end_line: int, end_column: int, target: string): void
```
