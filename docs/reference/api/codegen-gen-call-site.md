# GenCallSite

**Import:** `import system.codegen;`

## `GenCallSite`

A call of one of the generator's `@on_call` functions.

```dream
public class GenCallSite
```

## `id`

Stable identifier of this entry.

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

Source location associated with this entry.

```dream
public location: GenLocation
```

## `constructor`

Creates the snapshot model from the supplied values.

```dream
public constructor(
    id: string,
    callee: string,
    type_args: List<GenTypeRef>,
    arg_types: List<GenTypeRef>,
    location: GenLocation
)
```
