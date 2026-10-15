# GeneratorAttribute, IncrementalAttribute, OnAttributeAttribute, OnCallAttribute, SyntaxBlockAttribute, UsesToolAttribute

**Import:** `import system.codegen;`

## `public struct GeneratorAttribute`

Marks `fun name(ctx: GenContext)` as a source generator (`system.codegen`). The function name is the generator name.

```dream
public struct GeneratorAttribute
```

## `public struct IncrementalAttribute`

Declares a generator pure over its snapshot, so the compiler may replay a cached result instead of running it.

```dream
public struct IncrementalAttribute
```

## `public struct OnAttributeAttribute`

Runs the generator when a declaration carries one of the named `@attribute` types.

```dream
public struct OnAttributeAttribute
```

## `public declaration: DeclarationPath`

```dream
public declaration: DeclarationPath
```

## `public additional_declarations: List<DeclarationPath>`

```dream
public additional_declarations: List<DeclarationPath>
```

## `public struct OnCallAttribute`

Runs the generator for call sites of the named functions or methods (`Type.method`).

```dream
public struct OnCallAttribute
```

## `public declaration: DeclarationPath`

```dream
public declaration: DeclarationPath
```

## `public additional_declarations: List<DeclarationPath>`

```dream
public additional_declarations: List<DeclarationPath>
```

## `public struct SyntaxBlockAttribute`

Makes the generator expand `name { ... }` syntax blocks, where `name` is the generator's own name.

```dream
public struct SyntaxBlockAttribute
```

## `public struct UsesToolAttribute`

Declares a pinned compiler-run tool that this generator may request.

```dream
public struct UsesToolAttribute
```

## `public name: string`

```dream
public name: string
```
