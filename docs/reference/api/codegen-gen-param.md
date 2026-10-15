# GenParam

**Import:** `import system.codegen;`

## `public class GenParam`

```dream
public class GenParam
```

## `public name: string`

```dream
public name: string
```

## `public ty: GenTypeRef`

```dream
public ty: GenTypeRef
```

## `public attributes: List<GenAttribute>`

```dream
public attributes: List<GenAttribute>
```

## `public constructor(name: string, ty: GenTypeRef, attributes: List<GenAttribute>)`

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
