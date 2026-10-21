# GenField

**Import:** `import system.codegen;`

## `GenField`

A field of a class, struct or union variant.

```dream
public class GenField
```

## `id`

Identity of this model node; pass it to `GenContext.error_at` to report a diagnostic here.

```dream
public id: string
```

## `name`

Field name as declared.

```dream
public name: string
```

## `ty`

Declared type of the field.

```dream
public ty: GenTypeRef
```

## `visibility`

Declared access level of this symbol.

```dream
public visibility: string
```

## `attributes`

Attributes attached to this declaration.

```dream
public attributes: List<GenAttribute>
```

## `location`

Where the field is written in source.

```dream
public location: GenLocation
```

## `constructor`

Creates a `GenField` from its parts. Generators normally receive these from `GenContext` rather than constructing them.

```dream
public constructor(
    id: string,
    name: string,
    ty: GenTypeRef,
    visibility: string,
    attributes: List<GenAttribute>,
    location: GenLocation
)
```

## `attribute`

The first `@A` on this field, decoded into the attribute struct.

```dream
public fun attribute<A>(): Option<A>
```

## `attributes_of`

Decodes every attached attribute of the requested type.

```dream
public fun attributes_of<A>(): List<A>
```

## `has`

Whether this field carries at least one `@A` attribute.

```dream
public fun has<A>(): bool
```
