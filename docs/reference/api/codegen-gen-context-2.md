# Gen Context

**Import:** `import system.codegen;`

## `find_type`

The index entry named `name`, if the program declares such a type.

```dream
public fun find_type(borrow name: string): Option<GenIndexEntry>
```

## `syntax_blocks`

`name { ... }` sites introduced by this generator's name (`@syntax_block` generators).

```dream
public fun syntax_blocks(): List<GenSyntaxBlock>
```

## `call_sites`

Calls of the functions named in this generator's `@on_call(...)`.

```dream
public fun call_sites(): List<GenCallSite>
```

## `options`

`dream.toml` `[[generators]].options`, decoded into the `@json` type `T`.

```dream
public fun options<T>(): Result<T, ParseError>
```

## `option_or`

One option as text (`fallback` when unset).

```dream
public fun option_or(borrow key: string, fallback: string): string
```

## `additional_files`

Returns the files listed in `[[generators]].additional_files` in `dream.toml`.

```dream
public fun additional_files(): List<GenFile>
```

## `replace`

Rewrites `block`'s site to the Dream expression `source`.

```dream
public fun replace(borrow block: GenSyntaxBlock, source: string): void
```

## `emit_file`

Adds a complete Dream source file at the generator-relative path `path` (`models.json.dream`). It is written under `generated/<entry>/<module-path>/<generator>/` and compiled with the program.

```dream
public fun emit_file(path: string, source: string): void
```

## `tools`

Declared tools with the versions and fingerprints used in this compilation.

```dream
public fun tools(): List<GenToolInfo>
```

## `tool_results`

Returns tool results from the previous generation round.

```dream
public fun tool_results(): List<GenToolResult>
```

## `run_tool`

Requests a declared tool. The generator runs again with the result in the next phase.  `id` names the request (no `/`); `inputs` are emitted artifact paths copied into the tool's scratch folder; `args` are passed as-is with that folder as the working directory. `outputs` are files whose contents the next phase reads through `tool_results()`. `opaque_outputs` are files the generator only sees as path, hash and size, for large or binary results it embeds (`emit_asset`) or publishes (`publish_tool_output`) as `tool:<id>/<path>` without reading them. All requests of a phase run in parallel; results keep request order.

```dream
public fun run_tool(
    id: string,
    tool: string,
    args: List<string>,
    inputs: List<string>,
    outputs: List<string>,
    opaque_outputs: List<string>
): void
```

## `publish_tool_output`

Copies a tool output, named `tool:<request id>/<path>`, to `path` next to the compiled program. Publishing the same path twice, or a path a compiler output uses, is an error.

```dream
public fun publish_tool_output(source: string, path: string): void
```

## `emit_asset`

Embeds an emitted artifact path, or a tool output named `tool:<request id>/<path>`, into every compilation target as the asset `name`, read at run time with `Assets.bytes(name)`.

```dream
public fun emit_asset(name: string, source: string): void
```

## `emit_asset`

Embeds an artifact or tool output for the selected target triples or families.

```dream
public fun emit_asset(name: string, source: string, targets: List<string>): void
```

## `consume`

Takes over a selected or referenced top-level user function: it stays type-checked but is not compiled, and calling it from ordinary code is an error. `help` is shown under that error to say what to use instead, for example `dispatch it with Shaders.blur.dispatch(...)`.

```dream
public fun consume(borrow f: GenFunction, help: string = ""): void
```

## `emit_artifact`

Emits a non-Dream file with source mappings under this generator's directory.

```dream
public fun emit_artifact(
    path: string,
    contents: string,
    borrow map: GenSourceMap,
    publish: bool = false
): void
```

## `emit_artifact`

Emits a binary file, such as packed GPU code, under this generator's directory. Binary artifacts have no source map; embed them with `emit_asset` or pass them to `run_tool` as inputs like text artifacts. With `publish`, the file is also copied beside the build output.

```dream
public fun emit_artifact(path: string, contents: byte[], publish: bool = false): void
```

## `error_in_artifact`

Reports an artifact location through its source map to the originating Dream node.

```dream
public fun error_in_artifact(path: string, line: int, column: int, message: string): void
```

## `emit_extend`

Extensions keep their declaring module, including its internal visibility and name scope.

```dream
public fun emit_extend(borrow decl: GenDecl, body: string): void
```

## `error_at`

An error at the node with identity `target` (any model node's `id`), or at the generator itself for an empty `target`.

```dream
public fun error_at(borrow target: string, message: string): void
```

## `warning_at`

Reports a warning at this model node.

```dream
public fun warning_at(borrow target: string, message: string): void
```

## `error`

Reports an error at this model node or at the generator when no node is supplied.

```dream
public fun error(message: string): void
```

## `error`

Reports an error at this model node or at the generator when no node is supplied.

```dream
public fun error(borrow block: GenSyntaxBlock, message: string): void
```

## `error`

Reports an error at this model node or at the generator when no node is supplied.

```dream
public fun error(borrow decl: GenDecl, message: string): void
```
