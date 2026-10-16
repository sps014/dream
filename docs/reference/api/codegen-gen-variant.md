# GenVariant

**Import:** `import system.codegen;`

## `GenVariant`

A choice declared by an enum or discriminated union.

```dream
public class GenVariant
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

## `fields`

Fields of this type, in declaration order.

```dream
public fields: List<GenField>
```

## `location`

Source location associated with this entry.

```dream
public location: GenLocation
```

## `constructor`

Creates the snapshot model from the supplied values.

```dream
public constructor(id: string, name: string, fields: List<GenField>, location: GenLocation)
```
