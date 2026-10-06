# GenField, GenVariant, GenParam, GenFunction, GenDecl, GenIndexEntry, GenSyntaxBlock, GenCallSite, GenFile

**Import:** `import system.codegen;`

Read the [usage guide](../stdlib/codegen.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](codegen-gen-decls.md)

## `class GenFunction`

A method, or a top-level function.

```dream
public class GenFunction
```

## `id: string`

```dream
public id: string
```

## `name: string`

```dream
public name: string
```

## `is_static: bool`

```dream
public is_static: bool
```

## `is_async: bool`

```dream
public is_async: bool
```

## `visibility: string`

```dream
public visibility: string
```

## `generics: List<string>`

```dream
public generics: List<string>
```

## `params: List<GenParam>`

```dream
public params: List<GenParam>
```

## `ret: GenTypeRef`

```dream
public ret: GenTypeRef
```

## `attributes: List<GenAttribute>`

```dream
public attributes: List<GenAttribute>
```

## `location: GenLocation`

```dream
public location: GenLocation
```

## `constructor`

```dream
public constructor(id: string, name: string, is_static: bool, is_async: bool, visibility: string, generics: List<string>, params: List<GenParam>, ret: GenTypeRef, attributes: List<GenAttribute>, location: GenLocation)
```

## `attribute`

```dream
public fun attribute<A>(): Option<A>
```

## `has`

```dream
public fun has<A>(): bool
```

## `class GenDecl`

A class, struct, union, enum or interface the generator's triggers selected.

```dream
public class GenDecl
```

## `id: string`

```dream
public id: string
```

## `name: string`

```dream
public name: string
```

## `kind: DeclKind`

```dream
public kind: DeclKind
```

## `module_name: string`

```dream
public module_name: string
```

## `visibility: string`

```dream
public visibility: string
```

## `location: GenLocation`

```dream
public location: GenLocation
```

## `generics: List<string>`

```dream
public generics: List<string>
```

## `interfaces: List<GenTypeRef>`

```dream
public interfaces: List<GenTypeRef>
```

## `attributes: List<GenAttribute>`

```dream
public attributes: List<GenAttribute>
```
