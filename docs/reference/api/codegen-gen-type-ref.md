# Gen Type Ref

**Import:** `import system.codegen;`

## `GenTypeRef`

A resolved type with structured arguments and declaration identity.

```dream
public class GenTypeRef
```

## `kind`

Category of this entry.

```dream
public kind: TypeRefKind
```

## `name`

`int`, `List`, `T`, or empty for arrays/tuples/functions.

```dream
public name: string
```

## `display`

Display spelling, including canonical module qualification for resolved names.

```dream
public display: string
```

## `mangled`

Unique type spelling that distinguishes concrete generic instantiations; useful for generated names.

```dream
public mangled: string
```

## `decl`

Identity of the named declaration (`::User`, `system.collections::List`), or empty.

```dream
public decl: string
```

## `args`

Type arguments; the element of an array; tuple members; function params then return.

```dream
public args: List<GenTypeRef>
```

## `constructor`

Creates the snapshot model from the supplied values.

```dream
public constructor(
    kind: TypeRefKind,
    name: string,
    display: string,
    mangled: string,
    decl: string,
    args: List<GenTypeRef>
)
```

## `unknown`

Returns the requested model information.

```dream
public static fun unknown(): GenTypeRef
```

## `primitive`

The scalar this type is, or `Primitive.NotPrimitive`.

```dream
public fun primitive(): Primitive
```

## `is_primitive`

Returns the requested model information.

```dream
public fun is_primitive(p: Primitive): bool
```

## `collection`

The standard collection this type is, or `CollectionKind.NotCollection`.

```dream
public fun collection(): CollectionKind
```

## `is_option`

Returns the requested model information.

```dream
public fun is_option(): bool
```

## `is_param`

Returns the requested model information.

```dream
public fun is_param(): bool
```

## `is_decl`

True when this names the declaration `decl_id` (see `declof`).

```dream
public fun is_decl(decl_id: string): bool
```

## `arg`

Type argument `i`, or an unknown type when there is none.

```dream
public fun arg(i: int): GenTypeRef
```

## `source_name`

Renders a type using local declaration names in a scope that imports those declarations.

```dream
public fun source_name(): string
```

## `to_string`

Returns the requested model information.

```dream
public override fun to_string(): string
```
