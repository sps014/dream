# GenVariant

**Import:** `import system.codegen;`

## `GenVariant`

A choice declared by an enum or discriminated union.

```dream
public class GenVariant
```

## `id`

Identity of this model node; pass it to `GenContext.error_at` to report a diagnostic here.

```dream
public id: string
```

## `name`

Variant name as declared.

```dream
public name: string
```

## `fields`

Fields of this type, in declaration order.

```dream
public fields: List<GenField>
```

## `location`

Where the variant is written in source.

```dream
public location: GenLocation
```

## `constructor`

Creates a `GenVariant` from its parts. Generators normally receive these from `GenContext` rather than constructing them.

```dream
public constructor(id: string, name: string, fields: List<GenField>, location: GenLocation)
```
