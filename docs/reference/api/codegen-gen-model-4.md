# Primitive, CollectionKind, TypeRefKind, DeclKind, GenLocation, GenTypeRef, GenAttrArg, GenAttribute, GenAttributes

**Import:** `import system.codegen;`

Read the [usage guide](../stdlib/codegen.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](codegen-gen-model.md)

## `long_arg`

```dream
public fun long_arg(i: int): long
```

## `double_arg`

```dream
public fun double_arg(i: int): double
```

## `float_arg`

```dream
public fun float_arg(i: int): float
```

## `bool_arg`

```dream
public fun bool_arg(i: int): bool
```

## `enum_member`

The member name of an enum argument (`Get` for `HttpMethod.Get`).

```dream
public fun enum_member(i: int): string
```

## `class GenAttributes`

Typed attribute access. `A` is an `@attribute` struct; the compiler gives every such type the statics these forward to.

```dream
public static class GenAttributes
```

## `id`

Returns the compiler-assigned attribute identifier for attribute type `A`.

```dream
public static extern fun id<A>(): string
```

## `decode`

Decodes the first matching attribute as `A`, returning `Option.None` when absent.

```dream
public static extern fun decode<A>(borrow attrs: List<GenAttribute>): Option<A>
```

## `decode_all`

Decodes all matching attributes as `A` in source order.

```dream
public static extern fun decode_all<A>(borrow attrs: List<GenAttribute>): List<A>
```

## `has`

```dream
public static fun has(borrow attrs: List<GenAttribute>, borrow id: string): bool
```
