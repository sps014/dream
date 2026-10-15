# GenFunction

**Import:** `import system.codegen;`

## `public class GenFunction`

A method, or a top-level function.

```dream
public class GenFunction
```

## `public id: string`

```dream
public id: string
```

## `public overload: string`

```dream
public overload: string
```

## `public name: string`

```dream
public name: string
```

## `public is_static: bool`

```dream
public is_static: bool
```

## `public is_async: bool`

```dream
public is_async: bool
```

## `public visibility: string`

```dream
public visibility: string
```

## `public generics: List<string>`

```dream
public generics: List<string>
```

## `public params: List<GenParam>`

```dream
public params: List<GenParam>
```

## `public ret: GenTypeRef`

```dream
public ret: GenTypeRef
```

## `public attributes: List<GenAttribute>`

```dream
public attributes: List<GenAttribute>
```

## `public location: GenLocation`

```dream
public location: GenLocation
```

## `public body: Option<GenBody>`

```dream
public body: Option<GenBody>
```

## `public constructor(id: string, name: string, is_static: bool, is_async: bool, visibility: string, generics: List<string>, params: List<GenParam>, ret: GenTypeRef, attributes: List<GenAttribute>, location: GenLocation, body: Option<GenBody>, overload: string)`

Creates the snapshot model from the supplied values.

```dream
public constructor(id: string, name: string, is_static: bool, is_async: bool, visibility: string, generics: List<string>, params: List<GenParam>, ret: GenTypeRef, attributes: List<GenAttribute>, location: GenLocation, body: Option<GenBody>, overload: string)
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
