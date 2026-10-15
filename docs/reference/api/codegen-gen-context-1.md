# GenContext

**Import:** `import system.codegen;`

[All sections](codegen-gen-context.md)

## `public class GenContext`

What a `@generator fun name(ctx: GenContext)` sees and produces. The compiler runs the generator's cached executable with `--generator <name> --snapshot <in> --result <out>`; the harness loads the context from the snapshot, calls the generator, and `finish()` writes the result the compiler validates and merges.

```dream
public class GenContext
```

## `public generator_name: string`

The running generator's name and `module::name` identity.

```dream
public generator_name: string
```

## `public generator_id: string`

```dream
public generator_id: string
```

## `public target: string`

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

## `phase`

The compiler-tool feedback round, starting at zero.

```dream
public fun phase(): int
```

## `referenced_decls`

Transitive dependencies of the declarations selected by this generator.

```dream
public fun referenced_decls(): List<GenDecl>
```

## `referenced_functions`

Top-level functions referenced by selected declarations or their dependencies.

```dream
public fun referenced_functions(): List<GenFunction>
```

## `find_decl`

Looks up a selected or referenced declaration by its compiler identity.

```dream
public fun find_decl(borrow id: string): Option<GenDecl>
```

## `find_function`

Looks up a selected or referenced function, including methods.

```dream
public fun find_function(borrow id: string): Option<GenFunction>
```

## `type_of`

The inferred type, or an explicit Unknown when pre-analysis could not resolve it.

```dream
public fun type_of(borrow expr: GenExpr): GenTypeRef
```

## `type_of`

The inferred type attached to a statement, or Unknown when it has no value type.

```dream
public fun type_of(borrow stmt: GenStmt): GenTypeRef
```

## `symbol_of`

The resolved symbol identity, or an empty string for an unresolved expression.

```dream
public fun symbol_of(borrow expr: GenExpr): string
```

## `find_symbol`

Looks up a signature view anywhere in the pre-generation compilation.

```dream
public fun find_symbol(borrow id: string): Option<GenSymbol>
```

## `compilation_errors`

Errors found before generation within declarations selected by this generator's triggers.

```dream
public fun compilation_errors(): List<GenCompilationError>
```

## `implementations_of`

Classes and structs implementing an interface, including inherited interface relations.

```dream
public fun implementations_of(borrow id: string): List<GenSymbol>
```

## `subtypes_of`

Signature views whose declared interface ancestry includes the given identity.

```dream
public fun subtypes_of(borrow id: string): List<GenSymbol>
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
