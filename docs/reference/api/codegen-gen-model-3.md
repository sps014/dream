# Primitive, CollectionKind, TypeRefKind, DeclKind, GenLocation, GenTypeRef, GenAttrArg, GenAttribute, GenAttributes

**Import:** `import system.codegen;`

Read the [usage guide](../stdlib/codegen.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](codegen-gen-model.md)

## `decl: string`

Identity of the named declaration (`::User`, `system.collections::List`), or empty.

```dream
public decl: string
```

## `args: List<GenTypeRef>`

Type arguments; the element of an array; tuple members; function params then return.

```dream
public args: List<GenTypeRef>
```

## `constructor`

```dream
public constructor(kind: TypeRefKind, name: string, display: string, mangled: string, decl: string, args: List<GenTypeRef>)
```

## `unknown`

```dream
public static fun unknown(): GenTypeRef
```

## `primitive`

The scalar this type is, or `Primitive.NotPrimitive`.

```dream
public fun primitive(): Primitive
```

## `is_primitive`

```dream
public fun is_primitive(p: Primitive): bool
```

## `collection`

The standard collection this type is, or `CollectionKind.NotCollection`.

```dream
public fun collection(): CollectionKind
```

## `is_option`

```dream
public fun is_option(): bool
```

## `is_param`

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

## `override`

```dream
public override fun to_string(): string
```

## `primitive_named`

`Primitive` for a Dream scalar type name (`int`, `string`, ...).

```dream
public fun primitive_named(name: string): Primitive
```

## `class GenAttrArg`

One attribute argument as written: `kind` is `string`, `int`, `float`, `double`, `bool` or `enum`; `value` is the unquoted string, literal text, or dotted enum path.

```dream
public class GenAttrArg
```

## `kind: string`

```dream
public kind: string
```

## `value: string`

```dream
public value: string
```

## `constructor`

```dream
public constructor(kind: string, value: string)
```

## `class GenAttribute`

One attribute use. Prefer the typed `GenAttributes.decode<A>()` (via `attribute<A>()` on a node); the positional readers below are what decoders are built from.

```dream
public class GenAttribute
```

## `id: string`

`module::name` of the `@attribute` type (`system.json::json`), or `builtin::name`.

```dream
public id: string
```

## `name: string`

```dream
public name: string
```

## `args: List<GenAttrArg>`

```dream
public args: List<GenAttrArg>
```

## `constructor`

```dream
public constructor(id: string, name: string, args: List<GenAttrArg>)
```

## `string_arg`

```dream
public fun string_arg(i: int): string
```

## `int_arg`

```dream
public fun int_arg(i: int): int
```
