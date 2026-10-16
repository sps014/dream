# GeneratorAttribute, IncrementalAttribute, OnAttributeAttribute, OnCallAttribute, SyntaxBlockAttribute, UsesToolAttribute

**Import:** `import system.codegen;`

## `GeneratorAttribute`

Marks `fun name(ctx: GenContext)` as a source generator (`system.codegen`). The function name is the generator name.

```dream
public struct GeneratorAttribute
```

## `IncrementalAttribute`

Declares that a generator depends only on its input snapshot, allowing unchanged results to be reused.

```dream
public struct IncrementalAttribute
```

## `OnAttributeAttribute`

Runs the generator when a declaration carries one of the named `@attribute` types.

```dream
public struct OnAttributeAttribute
```

## `declaration`

Declaration associated with this entry.

```dream
public declaration: DeclarationPath
```

## `additional_declarations`

Additional declarations made available to the generator.

```dream
public additional_declarations: List<DeclarationPath>
```

## `OnCallAttribute`

Runs the generator for call sites of the named functions or methods (`Type.method`).

```dream
public struct OnCallAttribute
```

## `declaration`

Declaration associated with this entry.

```dream
public declaration: DeclarationPath
```

## `additional_declarations`

Additional declarations made available to the generator.

```dream
public additional_declarations: List<DeclarationPath>
```

## `SyntaxBlockAttribute`

Makes the generator expand `name { ... }` syntax blocks, where `name` is the generator's own name.

```dream
public struct SyntaxBlockAttribute
```

## `UsesToolAttribute`

Declares a pinned external tool that the generator may request.

```dream
public struct UsesToolAttribute
```

## `name`

Name identifying this entry.

```dream
public name: string
```
