# GenDecl

**Import:** `import system.codegen;`

## `GenDecl`

A class, struct, union, enum or interface the generator's triggers selected.

```dream
public class GenDecl
```

## `id`

Identity of this model node; pass it to `GenContext.error_at` to report a diagnostic here.

```dream
public id: string
```

## `name`

Unqualified declaration name (`User`).

```dream
public name: string
```

## `kind`

Whether this declares a class, struct, union, enum, or interface.

```dream
public kind: DeclKind
```

## `module_name`

Fully qualified module containing the declaration.

```dream
public module_name: string
```

## `visibility`

Declared access level of this symbol.

```dream
public visibility: string
```

## `location`

Where the declaration is written in source.

```dream
public location: GenLocation
```

## `generics`

Names of the declared type parameters, in order.

```dream
public generics: List<string>
```

## `interfaces`

Interfaces implemented by the declaration.

```dream
public interfaces: List<GenTypeRef>
```

## `attributes`

Attributes attached to this declaration.

```dream
public attributes: List<GenAttribute>
```

## `fields`

Fields of this type, in declaration order.

```dream
public fields: List<GenField>
```

## `methods`

Methods declared by this type.

```dream
public methods: List<GenFunction>
```

## `variants`

Choices declared by this union or enum, in declaration order.

```dream
public variants: List<GenVariant>
```

## `constructor`

Creates a `GenDecl` from its parts. Generators normally receive these from `GenContext` rather than constructing them.

```dream
public constructor(
    id: string,
    name: string,
    kind: DeclKind,
    module_name: string,
    visibility: string,
    location: GenLocation,
    generics: List<string>,
    interfaces: List<GenTypeRef>,
    attributes: List<GenAttribute>,
    fields: List<GenField>,
    methods: List<GenFunction>,
    variants: List<GenVariant>
)
```

## `attribute`

The first `@A` attached to this declaration, decoded into `A`, or `None` when absent.

```dream
public fun attribute<A>(): Option<A>
```

## `has`

Whether this declaration carries at least one `@A` attribute.

```dream
public fun has<A>(): bool
```

## `self_type`

`Name` or `Name<T, U>`: the spelling of this type inside its own generic scope.

```dream
public fun self_type(): string
```
