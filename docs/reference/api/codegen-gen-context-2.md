# Gen Context

**Import:** `import system.codegen;`

## `public fun options<T>`

`dream.toml` `[[generators]].options`, decoded into the `@json` type `T`.

```dream
public fun options<T>(): Result<T, ParseError>
```

## `public fun option_or`

One option as text (`fallback` when unset).

```dream
public fun option_or(borrow key: string, fallback: string): string
```

## `public fun additional_files`

`dream.toml` `[[generators]].additional_files`, read by the compiler.

```dream
public fun additional_files(): List<GenFile>
```

## `public fun replace`

Rewrites `block`'s site to the Dream expression `source`.

```dream
public fun replace(borrow block: GenSyntaxBlock, source: string): void
```

## `public fun emit_file`

Adds a complete Dream source file at the generator-relative path `path` (`models.json.dream`). It is written under `.dream/generated/<entry>/<generator>/` and compiled with the program.

```dream
public fun emit_file(path: string, source: string): void
```

## `public fun tools`

Declared tools with the versions and fingerprints used in this compilation.

```dream
public fun tools(): List<GenToolInfo>
```

## `public fun tool_results`

Compiler-collected feedback from the previous phase.

```dream
public fun tool_results(): List<GenToolResult>
```

## `public fun run_tool`

Requests a declared tool; the compiler reruns this generator with its results.

```dream
public fun run_tool(id: string, tool: string, args: List<string>, inputs: List<string>, outputs: List<string>): void
```

## `public fun emit_asset`

Embeds an artifact or tool output into every compilation target.

```dream
public fun emit_asset(name: string, source: string): void
```

## `public fun emit_asset`

Embeds an artifact or tool output for the selected target triples or families.

```dream
public fun emit_asset(name: string, source: string, targets: List<string>): void
```

## `public fun consume`

Removes a selected or referenced user function from the compiled program.

```dream
public fun consume(borrow f: GenFunction): void
```

## `public fun emit_artifact`

Emits a non-Dream file with source mappings under this generator's directory.

```dream
public fun emit_artifact(path: string, contents: string, borrow map: GenSourceMap, publish: bool = false): void
```

## `public fun error_in_artifact`

Reports an artifact location through its source map to the originating Dream node.

```dream
public fun error_in_artifact(path: string, line: int, column: int, message: string): void
```

## `public fun emit_extend`

Extensions keep their declaring module, including its internal visibility and name scope.

```dream
public fun emit_extend(borrow decl: GenDecl, body: string): void
```

## `public fun error_at`

An error at the node with identity `target` (any model node's `id`), or at the generator itself for an empty `target`.

```dream
public fun error_at(borrow target: string, message: string): void
```

## `public fun warning_at`

Reports a warning at this model node.

```dream
public fun warning_at(borrow target: string, message: string): void
```

## `public fun error`

Reports an error at this model node or at the generator when no node is supplied.

```dream
public fun error(message: string): void
```

## `public fun error`

Reports an error at this model node or at the generator when no node is supplied.

```dream
public fun error(borrow block: GenSyntaxBlock, message: string): void
```

## `public fun error`

Reports an error at this model node or at the generator when no node is supplied.

```dream
public fun error(borrow decl: GenDecl, message: string): void
```

## `public fun error`

Reports an error at this model node or at the generator when no node is supplied.

```dream
public fun error(borrow field: GenField, message: string): void
```

## `public fun error`

Reports an error at this model node or at the generator when no node is supplied.

```dream
public fun error(borrow call: GenCallSite, message: string): void
```

## `public fun warning`

Reports a warning at this model node.

```dream
public fun warning(borrow decl: GenDecl, message: string): void
```

## `public fun has_errors`

Returns the requested model information.

```dream
public fun has_errors(): bool
```

## `public fun log`

A line shown with `dream -v` / `dream generate --explain`.

```dream
public fun log(message: string): void
```
