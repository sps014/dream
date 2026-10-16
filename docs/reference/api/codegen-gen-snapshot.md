# GenSnapshot

**Import:** `import system.codegen;`

## `GenSnapshot`

Decoded snapshot behind a `GenContext`.

```dream
public class GenSnapshot
```

## `version`

Version identifying this API or snapshot format.

```dream
public version: int
```

## `phase`

Current source-generation phase.

```dream
public phase: int
```

## `symbols`

Symbols available to the generator.

```dream
public symbols: List<GenSymbol>
```

## `compilation_errors`

Errors available for the generator to inspect.

```dream
public compilation_errors: List<GenCompilationError>
```

## `tools`

Tools available to the generator.

```dream
public tools: List<GenToolInfo>
```

## `tool_results`

Results of tool requests made by the generator.

```dream
public tool_results: List<GenToolResult>
```

## `referenced`

Referenced type declarations available to the generator.

```dream
public referenced: List<GenDecl>
```

## `referenced_functions`

Referenced function declarations available to the generator.

```dream
public referenced_functions: List<GenFunction>
```

## `generator`

Name of the generator being executed.

```dream
public generator: string
```

## `generator_id`

Stable identifier of the generator being executed.

```dream
public generator_id: string
```

## `target`

Target declaration or build target associated with this result.

```dream
public target: string
```

## `options`

Options supplied to this generator.

```dream
public options: JsonValue
```

## `files`

Files available to this operation.

```dream
public files: List<GenFile>
```

## `index`

Searchable declaration index available to the generator.

```dream
public index: List<GenIndexEntry>
```

## `decls`

Type declarations available to the generator.

```dream
public decls: List<GenDecl>
```

## `functions`

Function declarations available to the generator.

```dream
public functions: List<GenFunction>
```

## `blocks`

Syntax blocks available to the generator.

```dream
public blocks: List<GenSyntaxBlock>
```

## `calls`

Function call sites available to the generator.

```dream
public calls: List<GenCallSite>
```

## `constructor`

Creates the snapshot model from the supplied values.

```dream
public constructor(root: JsonValue)
```
