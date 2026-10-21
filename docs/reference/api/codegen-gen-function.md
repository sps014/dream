# GenFunction

**Import:** `import system.codegen;`

## `GenFunction`

A method, or a top-level function.

```dream
public class GenFunction
```

## `id`

Identity of this model node; pass it to `GenContext.error_at` to report a diagnostic here.

```dream
public id: string
```

## `overload`

Identifier distinguishing this function's overload.

```dream
public overload: string
```

## `name`

Function or method name as declared.

```dream
public name: string
```

## `is_static`

Whether the member belongs to the type rather than an instance.

```dream
public is_static: bool
```

## `is_async`

Whether this function returns an asynchronous result.

```dream
public is_async: bool
```

## `visibility`

Declared access level of this symbol.

```dream
public visibility: string
```

## `generics`

Names of the declared type parameters, in order.

```dream
public generics: List<string>
```

## `params`

Function parameters in declaration order.

```dream
public params: List<GenParam>
```

## `ret`

Declared return type of the function.

```dream
public ret: GenTypeRef
```

## `attributes`

Attributes attached to this declaration.

```dream
public attributes: List<GenAttribute>
```

## `location`

Where the function is written in source.

```dream
public location: GenLocation
```

## `body`

Function body, when source is available.

```dream
public body: Option<GenBody>
```

## `constructor`

Creates a `GenFunction` from its parts. Generators normally receive these from `GenContext` rather than constructing them.

```dream
public constructor(
    id: string,
    name: string,
    is_static: bool,
    is_async: bool,
    visibility: string,
    generics: List<string>,
    params: List<GenParam>,
    ret: GenTypeRef,
    attributes: List<GenAttribute>,
    location: GenLocation,
    body: Option<GenBody>,
    overload: string
)
```

## `attribute`

The first `@A` attached to this function, decoded into `A`, or `None` when absent.

```dream
public fun attribute<A>(): Option<A>
```

## `has`

Whether this function carries at least one `@A` attribute.

```dream
public fun has<A>(): bool
```
