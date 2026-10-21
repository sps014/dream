# GenCallSite

**Import:** `import system.codegen;`

## `GenCallSite`

A call of one of the generator's `@on_call` functions.

```dream
public class GenCallSite
```

## `id`

Identity of this model node; pass it to `GenContext.error_at` to report a diagnostic here.

```dream
public id: string
```

## `callee`

Identity of the callee (compare with `declof(Json.serialize)`).

```dream
public callee: string
```

## `type_args`

Concrete type arguments, in parameter order.

```dream
public type_args: List<GenTypeRef>
```

## `arg_types`

Statically known argument types (`TypeRefKind.Unknown` when not inferable).

```dream
public arg_types: List<GenTypeRef>
```

## `location`

Where the call is written in source.

```dream
public location: GenLocation
```

## `constructor`

Creates a `GenCallSite` from its parts. Generators normally receive these from `GenContext` rather than constructing them.

```dream
public constructor(
    id: string,
    callee: string,
    type_args: List<GenTypeRef>,
    arg_types: List<GenTypeRef>,
    location: GenLocation
)
```
