# GenField

**Import:** `import system.codegen;`

## `public class GenField`

A field of a class, struct or union variant.

```dream
public class GenField
```

## `public id: string`

```dream
public id: string
```

## `public name: string`

```dream
public name: string
```

## `public ty: GenTypeRef`

```dream
public ty: GenTypeRef
```

## `public visibility: string`

```dream
public visibility: string
```

## `public attributes: List<GenAttribute>`

```dream
public attributes: List<GenAttribute>
```

## `public location: GenLocation`

```dream
public location: GenLocation
```

## `public constructor(id: string, name: string, ty: GenTypeRef, visibility: string, attributes: List<GenAttribute>, location: GenLocation)`

Creates the snapshot model from the supplied values.

```dream
public constructor(id: string, name: string, ty: GenTypeRef, visibility: string, attributes: List<GenAttribute>, location: GenLocation)
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
