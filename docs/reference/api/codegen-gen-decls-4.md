# GenField, GenVariant, GenParam, GenFunction, GenDecl, GenIndexEntry, GenSyntaxBlock, GenCallSite, GenFile

**Import:** `import system.codegen;`

Read the [usage guide](../stdlib/codegen.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](codegen-gen-decls.md)

## `location: GenLocation`

```dream
public location: GenLocation
```

## `constructor`

```dream
public constructor(id: string, callee: string, type_args: List<GenTypeRef>, arg_types: List<GenTypeRef>, location: GenLocation)
```

## `class GenFile`

A `dream.toml` `additional_files` entry.

```dream
public class GenFile
```

## `path: string`

```dream
public path: string
```

## `contents: string`

```dream
public contents: string
```

## `constructor`

```dream
public constructor(path: string, contents: string)
```
