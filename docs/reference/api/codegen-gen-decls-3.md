# GenField, GenVariant, GenParam, GenFunction, GenDecl, GenIndexEntry, GenSyntaxBlock, GenCallSite, GenFile

**Import:** `import system.codegen;`

Read the [usage guide](../stdlib/codegen.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](codegen-gen-decls.md)

## `fields: List<GenField>`

```dream
public fields: List<GenField>
```

## `methods: List<GenFunction>`

```dream
public methods: List<GenFunction>
```

## `variants: List<GenVariant>`

```dream
public variants: List<GenVariant>
```

## `constructor`

```dream
public constructor(id: string, name: string, kind: DeclKind, module_name: string, visibility: string, location: GenLocation, generics: List<string>, interfaces: List<GenTypeRef>, attributes: List<GenAttribute>, fields: List<GenField>, methods: List<GenFunction>, variants: List<GenVariant>)
```

## `attribute`

```dream
public fun attribute<A>(): Option<A>
```

## `has`

```dream
public fun has<A>(): bool
```

## `self_type`

`Name` or `Name<T, U>`: the spelling of this type inside its own generic scope.

```dream
public fun self_type(): string
```

## `class GenIndexEntry`

Every type in the program (user and stdlib), names and kinds only.

```dream
public class GenIndexEntry
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

## `constructor`

```dream
public constructor(id: string, name: string, kind: DeclKind)
```

## `class GenSyntaxBlock`

A `name { ... }` site of this generator (`@syntax_block` generators only).

```dream
public class GenSyntaxBlock
```

## `id: string`

```dream
public id: string
```

## `name: string`

```dream
public name: string
```

## `body: string`

Reconstructed body text; splices appear as `{expr}`.

```dream
public body: string
```

## `splices: List<string>`

Dream source of each `{ ... }` splice, in order.

```dream
public splices: List<string>
```

## `location: GenLocation`

```dream
public location: GenLocation
```

## `constructor`

```dream
public constructor(id: string, name: string, body: string, splices: List<string>, location: GenLocation)
```

## `class GenCallSite`

A call of one of the generator's `@on_call` functions.

```dream
public class GenCallSite
```

## `id: string`

```dream
public id: string
```

## `callee: string`

Identity of the callee (compare with `declof(Json.serialize)`).

```dream
public callee: string
```

## `type_args: List<GenTypeRef>`

```dream
public type_args: List<GenTypeRef>
```

## `arg_types: List<GenTypeRef>`

Statically known argument types (`TypeRefKind.Unknown` when not inferable).

```dream
public arg_types: List<GenTypeRef>
```
