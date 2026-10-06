# GenField, GenVariant, GenParam, GenFunction, GenDecl, GenIndexEntry, GenSyntaxBlock, GenCallSite, GenFile

**Import:** `import system.codegen;`

Read the [usage guide](../stdlib/codegen.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](codegen-gen-decls.md)

## `class GenField`

A field of a class, struct or union variant.

```dream
public class GenField
```

## `id: string`

```dream
public id: string
```

## `name: string`

```dream
public name: string
```

## `ty: GenTypeRef`

```dream
public ty: GenTypeRef
```

## `visibility: string`

```dream
public visibility: string
```

## `attributes: List<GenAttribute>`

```dream
public attributes: List<GenAttribute>
```

## `location: GenLocation`

```dream
public location: GenLocation
```

## `constructor`

```dream
public constructor(id: string, name: string, ty: GenTypeRef, visibility: string, attributes: List<GenAttribute>, location: GenLocation)
```

## `attribute`

The first `@A` on this field, decoded into the attribute struct.

```dream
public fun attribute<A>(): Option<A>
```

## `attributes_of`

```dream
public fun attributes_of<A>(): List<A>
```

## `has`

```dream
public fun has<A>(): bool
```

## `class GenVariant`

```dream
public class GenVariant
```

## `id: string`

```dream
public id: string
```

## `name: string`

```dream
public name: string
```

## `fields: List<GenField>`

```dream
public fields: List<GenField>
```

## `location: GenLocation`

```dream
public location: GenLocation
```

## `constructor`

```dream
public constructor(id: string, name: string, fields: List<GenField>, location: GenLocation)
```

## `class GenParam`

```dream
public class GenParam
```

## `name: string`

```dream
public name: string
```

## `ty: GenTypeRef`

```dream
public ty: GenTypeRef
```

## `attributes: List<GenAttribute>`

```dream
public attributes: List<GenAttribute>
```

## `constructor`

```dream
public constructor(name: string, ty: GenTypeRef, attributes: List<GenAttribute>)
```

## `attribute`

```dream
public fun attribute<A>(): Option<A>
```

## `has`

```dream
public fun has<A>(): bool
```
