# GenSnapshot

**Import:** `import system.codegen;`

## `public class GenSnapshot`

Decoded snapshot behind a `GenContext`.

```dream
public class GenSnapshot
```

## `public version: int`

```dream
public version: int
```

## `public phase: int`

```dream
public phase: int
```

## `public symbols: List<GenSymbol>`

```dream
public symbols: List<GenSymbol>
```

## `public compilation_errors: List<GenCompilationError>`

```dream
public compilation_errors: List<GenCompilationError>
```

## `public tools: List<GenToolInfo>`

```dream
public tools: List<GenToolInfo>
```

## `public tool_results: List<GenToolResult>`

```dream
public tool_results: List<GenToolResult>
```

## `public referenced: List<GenDecl>`

```dream
public referenced: List<GenDecl>
```

## `public referenced_functions: List<GenFunction>`

```dream
public referenced_functions: List<GenFunction>
```

## `public generator: string`

```dream
public generator: string
```

## `public generator_id: string`

```dream
public generator_id: string
```

## `public target: string`

```dream
public target: string
```

## `public options: JsonValue`

```dream
public options: JsonValue
```

## `public files: List<GenFile>`

```dream
public files: List<GenFile>
```

## `public index: List<GenIndexEntry>`

```dream
public index: List<GenIndexEntry>
```

## `public decls: List<GenDecl>`

```dream
public decls: List<GenDecl>
```

## `public functions: List<GenFunction>`

```dream
public functions: List<GenFunction>
```

## `public blocks: List<GenSyntaxBlock>`

```dream
public blocks: List<GenSyntaxBlock>
```

## `public calls: List<GenCallSite>`

```dream
public calls: List<GenCallSite>
```

## `public constructor(root: JsonValue)`

Creates the snapshot model from the supplied values.

```dream
public constructor(root: JsonValue)
```
