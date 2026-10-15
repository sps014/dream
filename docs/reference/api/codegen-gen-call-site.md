# GenCallSite

**Import:** `import system.codegen;`

## `public class GenCallSite`

A call of one of the generator's `@on_call` functions.

```dream
public class GenCallSite
```

## `public id: string`

```dream
public id: string
```

## `public callee: string`

Identity of the callee (compare with `declof(Json.serialize)`).

```dream
public callee: string
```

## `public type_args: List<GenTypeRef>`

```dream
public type_args: List<GenTypeRef>
```

## `public arg_types: List<GenTypeRef>`

Statically known argument types (`TypeRefKind.Unknown` when not inferable).

```dream
public arg_types: List<GenTypeRef>
```

## `public location: GenLocation`

```dream
public location: GenLocation
```

## `public constructor(id: string, callee: string, type_args: List<GenTypeRef>, arg_types: List<GenTypeRef>, location: GenLocation)`

Creates the snapshot model from the supplied values.

```dream
public constructor(id: string, callee: string, type_args: List<GenTypeRef>, arg_types: List<GenTypeRef>, location: GenLocation)
```
