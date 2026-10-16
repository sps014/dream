# Primitive, CollectionKind, TypeRefKind, DeclKind

**Import:** `import system.codegen;`

[All sections](codegen-gen-model.md)

## `Named`

Identifies named types.

```dream
Named
```

## `Param`

Identifies param types.

```dream
Param
```

## `Array`

Identifies array types.

```dream
Array
```

## `Tuple`

Identifies tuple types.

```dream
Tuple
```

## `Function`

Identifies function types.

```dream
Function
```

## `Void`

Identifies void types.

```dream
Void
```

## `Unknown`

Identifies unknown types.

```dream
Unknown
```

## `DeclKind`

What a `GenDecl` declares.

```dream
public enum DeclKind
```

## `Class`

Identifies class declarations.

```dream
Class
```

## `Struct`

Identifies struct declarations.

```dream
Struct
```

## `Union`

Identifies union declarations.

```dream
Union
```

## `Enum`

Identifies enum declarations.

```dream
Enum
```

## `Interface`

Identifies interface declarations.

```dream
Interface
```

## `primitive_named`

`Primitive` for a Dream scalar type name (`int`, `string`, ...).

```dream
public fun primitive_named(name: string): Primitive
```
