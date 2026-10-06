# Primitive, CollectionKind, TypeRefKind, DeclKind, GenLocation, GenTypeRef, GenAttrArg, GenAttribute, GenAttributes

**Import:** `import system.codegen;`

Read the [usage guide](../stdlib/codegen.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](codegen-gen-model.md)

## `Named`

```dream
Named
```

## `Param`

```dream
Param
```

## `Array`

```dream
Array
```

## `Tuple`

```dream
Tuple
```

## `Function`

```dream
Function
```

## `Void`

```dream
Void
```

## `Unknown`

```dream
Unknown
```

## `enum DeclKind`

What a `GenDecl` declares.

```dream
public enum DeclKind
```

## `Class`

```dream
Class
```

## `Struct`

```dream
Struct
```

## `Union`

```dream
Union
```

## `Enum`

```dream
Enum
```

## `Interface`

```dream
Interface
```

## `class GenLocation`

```dream
public class GenLocation
```

## `file: string`

Project-relative path (`src/models.dream`), or `<std>/...` for stdlib sources.

```dream
public file: string
```

## `line: int`

```dream
public line: int
```

## `column: int`

```dream
public column: int
```

## `constructor`

```dream
public constructor(file: string, line: int, column: int)
```

## `override`

```dream
public override fun to_string(): string
```

## `class GenTypeRef`

A resolved type: `kind` + structured `args` instead of a type string to parse.

```dream
public class GenTypeRef
```

## `kind: TypeRefKind`

```dream
public kind: TypeRefKind
```

## `name: string`

`int`, `List`, `T`, or empty for arrays/tuples/functions.

```dream
public name: string
```

## `display: string`

Dream spelling (`List<int>`, `User[]`), usable verbatim in generated code.

```dream
public display: string
```

## `mangled: string`

Compiler-mangled spelling (`List_int`), unique per instantiation; handy for helper names.

```dream
public mangled: string
```
