# GenParam

**Import:** `import system.codegen;`

## `GenParam`

A function parameter and its type and attributes.

```dream
public class GenParam
```

## `name`

Parameter name as declared.

```dream
public name: string
```

## `ty`

Declared type of the parameter.

```dream
public ty: GenTypeRef
```

## `is_ref`

Whether this is a `ref` parameter, so assignments to it in the body are visible to the caller.

```dream
public is_ref: bool
```

## `attributes`

Attributes attached to this declaration.

```dream
public attributes: List<GenAttribute>
```

## `constructor`

Creates a `GenParam` from its parts. Generators normally receive these from `GenContext` rather than constructing them.

```dream
public constructor(name: string, ty: GenTypeRef, is_ref: bool, attributes: List<GenAttribute>)
```

## `attribute`

The first `@A` attached to this parameter, decoded into `A`, or `None` when absent.

```dream
public fun attribute<A>(): Option<A>
```

## `has`

Whether this parameter carries at least one `@A` attribute.

```dream
public fun has<A>(): bool
```
