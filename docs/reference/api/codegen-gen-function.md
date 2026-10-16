# GenFunction

**Import:** `import system.codegen;`

## `GenFunction`

A method, or a top-level function.

```dream
public class GenFunction
```

## `id`

Stable identifier of this entry.

```dream
public id: string
```

## `overload`

Identifier distinguishing this function's overload.

```dream
public overload: string
```

## `name`

Name identifying this entry.

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

Source location associated with this entry.

```dream
public location: GenLocation
```

## `body`

Function body, when source is available.

```dream
public body: Option<GenBody>
```

## `constructor`

Creates the snapshot model from the supplied values.

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

Returns the requested model information.

```dream
public fun attribute<A>(): Option<A>
```

## `has`

Tests whether the requested attribute is attached to this model node.

```dream
public fun has<A>(): bool
```
