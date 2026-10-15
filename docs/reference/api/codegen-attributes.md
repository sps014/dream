# system.codegen: Attribute declarations

Import `system.codegen` to use these declarations.

```dream
public struct GeneratorAttribute
```

Marks `fun name(ctx: GenContext)` as a source generator (`system.codegen`). The function name is the generator name.

```dream
public struct IncrementalAttribute
```

Declares a generator pure over its snapshot, so the compiler may replay a cached result instead of running it.

```dream
public struct OnAttributeAttribute
```

Runs the generator when a declaration carries one of the named `@attribute` types.

```dream
public declaration: DeclarationPath
```

```dream
public additional_declarations: List<DeclarationPath>
```

```dream
public struct OnCallAttribute
```

Runs the generator for call sites of the named functions or methods (`Type.method`).

```dream
public declaration: DeclarationPath
```

```dream
public additional_declarations: List<DeclarationPath>
```

```dream
public struct SyntaxBlockAttribute
```

Makes the generator expand `name { ... }` syntax blocks, where `name` is the generator's own name.
