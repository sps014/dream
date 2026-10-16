# GenParam

**Import:** `import system.codegen;`

## `GenParam`

A function parameter and its type and attributes.

```dream
public class GenParam
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

## `attributes`

Attributes attached to this declaration.

```dream
public attributes: List<GenAttribute>
```

## `constructor`

Creates the snapshot model from the supplied values.

```dream
public constructor(name: string, ty: GenTypeRef, attributes: List<GenAttribute>)
```

## `attribute`

Returns the requested model information.

```dream
public fun attribute<A>(): Option<A>
```

## `has`

Tests whether the requested attribute is attached to this model node.

```dream
public fun has<A>(): bool
```
