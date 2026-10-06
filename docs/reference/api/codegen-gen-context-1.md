# GenContext

**Import:** `import system.codegen;`

Read the [usage guide](../stdlib/codegen.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](codegen-gen-context.md)

## `class GenContext`

What a `@generator fun name(ctx: GenContext)` sees and produces. The compiler runs the generator's cached executable with `--generator <name> --snapshot <in> --result <out>`; the harness loads the context from the snapshot, calls the generator, and `finish()` writes the result the compiler validates and merges.

```dream
public class GenContext
```

## `generator_name: string`

The running generator's name and `module::name` identity.

```dream
public generator_name: string
```

## `generator_id: string`

```dream
public generator_id: string
```

## `target: string`

Target triple of the compile (`aarch64-apple-darwin`, `wasm32-unknown-wasi`, ...).

```dream
public target: string
```

## `load`

Reads `--snapshot` / `--result` from the program arguments.

```dream
public static async fun load(args: string[]): Result<GenContext, string>
```

## `types_with`

Declarations carrying `@A` (on the declaration or on one of its members).

```dream
public fun types_with<A>(): List<GenDecl>
```

## `functions_with`

Top-level functions carrying `@A` (on the function or a parameter).

```dream
public fun functions_with<A>(): List<GenFunction>
```

## `decls`

Every declaration selected by this generator's `@on_attribute` triggers.

```dream
public fun decls(): List<GenDecl>
```

## `type_index`

Every type in the program, by name and kind.

```dream
public fun type_index(): List<GenIndexEntry>
```

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

`dream.toml` `[[generators]].additional_files`, read by the compiler.

```dream
public fun additional_files(): List<GenFile>
```

## `replace`

Rewrites `block`'s site to the Dream expression `source`.

```dream
public fun replace(borrow block: GenSyntaxBlock, source: string): void
```

## `emit_file`

Adds a complete Dream source file at the generator-relative path `path` (`models.json.dream`). It is written under `.dream/generated/<entry>/<generator>/` and compiled with the program.

```dream
public fun emit_file(path: string, source: string): void
```

## `emit_extend`

Adds `extend <decl> { <body> }` to this generator's `extends.dream` file.

```dream
public fun emit_extend(borrow decl: GenDecl, body: string): void
```

## `error_at`

An error at the node with identity `target` (any model node's `id`), or at the generator itself for an empty `target`.

```dream
public fun error_at(borrow target: string, message: string): void
```

## `warning_at`

```dream
public fun warning_at(borrow target: string, message: string): void
```

## `error`

```dream
public fun error(message: string): void
```

## `error`

```dream
public fun error(borrow block: GenSyntaxBlock, message: string): void
```

## `error`

```dream
public fun error(borrow decl: GenDecl, message: string): void
```

## `error`

```dream
public fun error(borrow field: GenField, message: string): void
```
