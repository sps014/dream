# Gen Type Ref

**Import:** `import system.codegen;`

## `public class GenTypeRef`

A resolved type with structured arguments and declaration identity.

```dream
public class GenTypeRef
```

## `public kind`

```dream
public kind: TypeRefKind
```

## `public name`

`int`, `List`, `T`, or empty for arrays/tuples/functions.

```dream
public name: string
```

## `public display`

Display spelling, including canonical module qualification for resolved names.

```dream
public display: string
```

## `public mangled`

Compiler-mangled spelling (`List_int`), unique per instantiation; handy for helper names.

```dream
public mangled: string
```

## `public decl`

Identity of the named declaration (`::User`, `system.collections::List`), or empty.

```dream
public decl: string
```

## `public args`

Type arguments; the element of an array; tuple members; function params then return.

```dream
public args: List<GenTypeRef>
```

## `public constructor`

Creates the snapshot model from the supplied values.

```dream
public constructor(kind: TypeRefKind, name: string, display: string, mangled: string, decl: string, args: List<GenTypeRef>)
```

## `public static fun unknown`

Returns the requested model information.

```dream
public static fun unknown(): GenTypeRef
```

## `public fun primitive`

The scalar this type is, or `Primitive.NotPrimitive`.

```dream
public fun primitive(): Primitive
```

## `public fun is_primitive`

Returns the requested model information.

```dream
public fun is_primitive(p: Primitive): bool
```

## `public fun collection`

The standard collection this type is, or `CollectionKind.NotCollection`.

```dream
public fun collection(): CollectionKind
```

## `public fun is_option`

Returns the requested model information.

```dream
public fun is_option(): bool
```

## `public fun is_param`

Returns the requested model information.

```dream
public fun is_param(): bool
```

## `public fun is_decl`

True when this names the declaration `decl_id` (see `declof`).

```dream
public fun is_decl(decl_id: string): bool
```

## `public fun arg`

Type argument `i`, or an unknown type when there is none.

```dream
public fun arg(i: int): GenTypeRef
```

## `public fun source_name`

Renders a type using local declaration names in a scope that imports those declarations.

```dream
public fun source_name(): string
```

## `public override fun to_string`

Returns the requested model information.

```dream
public override fun to_string(): string
```
