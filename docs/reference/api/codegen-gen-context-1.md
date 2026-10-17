# Gen Context

**Import:** `import system.codegen;`

## `GenContext`

Input and output of a source generator. Inspect selected declarations, request tools, report diagnostics, and add or replace generated source through this context.

```dream
public class GenContext
```

## `generator_name`

The running generator's name and `module::name` identity.

```dream
public generator_name: string
```

## `generator_id`

Stable identifier of the generator being executed.

```dream
public generator_id: string
```

## `target`

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

## `modules_with`

User modules whose `module` declaration carries `@A`, in source order.

```dream
public fun modules_with<A>(): List<GenModule>
```

## `module_of`

The trigger-carrying module that declares `decl`, so module-level settings can be merged with the declaration's own attributes. `None` when its module carries no trigger.

```dream
public fun module_of(borrow decl: GenDecl): Option<GenModule>
```

## `module_of`

The trigger-carrying module that declares function or method `f`, or `None` when its module carries no trigger attribute.

```dream
public fun module_of(borrow f: GenFunction): Option<GenModule>
```

## `decls`

Every declaration selected by this generator's `@on_attribute` triggers.

```dream
public fun decls(): List<GenDecl>
```

## `phase`

Current generation round, starting at zero and increasing after tool requests.

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

Looks up a selected or referenced declaration by its stable identifier.

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
