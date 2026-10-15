# Primitive, CollectionKind, TypeRefKind, DeclKind

**Import:** `import system.codegen;`

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

## `public enum DeclKind`

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

## `primitive_named`

`Primitive` for a Dream scalar type name (`int`, `string`, ...).

```dream
public fun primitive_named(name: string): Primitive
```
