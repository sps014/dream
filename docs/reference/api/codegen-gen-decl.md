# GenDecl

**Import:** `import system.codegen;`

## `public class GenDecl`

A class, struct, union, enum or interface the generator's triggers selected.

```dream
public class GenDecl
```

## `public id: string`

```dream
public id: string
```

## `public name: string`

```dream
public name: string
```

## `public kind: DeclKind`

```dream
public kind: DeclKind
```

## `public module_name: string`

```dream
public module_name: string
```

## `public visibility: string`

```dream
public visibility: string
```

## `public location: GenLocation`

```dream
public location: GenLocation
```

## `public generics: List<string>`

```dream
public generics: List<string>
```

## `public interfaces: List<GenTypeRef>`

```dream
public interfaces: List<GenTypeRef>
```

## `public attributes: List<GenAttribute>`

```dream
public attributes: List<GenAttribute>
```

## `public fields: List<GenField>`

```dream
public fields: List<GenField>
```

## `public methods: List<GenFunction>`

```dream
public methods: List<GenFunction>
```

## `public variants: List<GenVariant>`

```dream
public variants: List<GenVariant>
```

## `public constructor(id: string, name: string, kind: DeclKind, module_name: string, visibility: string, location: GenLocation, generics: List<string>, interfaces: List<GenTypeRef>, attributes: List<GenAttribute>, fields: List<GenField>, methods: List<GenFunction>, variants: List<GenVariant>)`

Creates the snapshot model from the supplied values.

```dream
public constructor(id: string, name: string, kind: DeclKind, module_name: string, visibility: string, location: GenLocation, generics: List<string>, interfaces: List<GenTypeRef>, attributes: List<GenAttribute>, fields: List<GenField>, methods: List<GenFunction>, variants: List<GenVariant>)
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

## `self_type`

`Name` or `Name<T, U>`: the spelling of this type inside its own generic scope.

```dream
public fun self_type(): string
```
