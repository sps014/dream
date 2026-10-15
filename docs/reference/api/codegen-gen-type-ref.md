# GenTypeRef

**Import:** `import system.codegen;`

## `public class GenTypeRef`

A resolved type with structured arguments and declaration identity.

```dream
public class GenTypeRef
```

## `public kind: TypeRefKind`

```dream
public kind: TypeRefKind
```

## `public name: string`

`int`, `List`, `T`, or empty for arrays/tuples/functions.

```dream
public name: string
```

## `public display: string`

Dream spelling (`List<int>`, `User[]`), usable verbatim in generated code.

```dream
public display: string
```

## `public mangled: string`

Compiler-mangled spelling (`List_int`), unique per instantiation; handy for helper names.

```dream
public mangled: string
```

## `public decl: string`

Identity of the named declaration (`::User`, `system.collections::List`), or empty.

```dream
public decl: string
```

## `public args: List<GenTypeRef>`

Type arguments; the element of an array; tuple members; function params then return.

```dream
public args: List<GenTypeRef>
```

## `public constructor(kind: TypeRefKind, name: string, display: string, mangled: string, decl: string, args: List<GenTypeRef>)`

Creates the snapshot model from the supplied values.

```dream
public constructor(kind: TypeRefKind, name: string, display: string, mangled: string, decl: string, args: List<GenTypeRef>)
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

## `to_string`

Returns the requested model information.

```dream
public override fun to_string(): string
```
