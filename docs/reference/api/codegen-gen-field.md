# GenField

**Import:** `import system.codegen;`

## `GenField`

A field of a class, struct or union variant.

```dream
public class GenField
```

## `id`

Stable identifier of this entry.

```dream
public id: string
```

## `name`

Name identifying this entry.

```dream
public name: string
```

## `ty`

Type of this declaration or expression.

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

Source location associated with this entry.

```dream
public location: GenLocation
```

## `constructor`

Creates the snapshot model from the supplied values.

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

Tests whether the requested attribute is attached to this model node.

```dream
public fun has<A>(): bool
```
