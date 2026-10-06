# Primitive, CollectionKind, TypeRefKind, DeclKind, GenLocation, GenTypeRef, GenAttrArg, GenAttribute, GenAttributes

**Import:** `import system.codegen;`

Read the [usage guide](../stdlib/codegen.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](codegen-gen-model.md)

## `enum Primitive`

Built-in scalar types.

```dream
public enum Primitive
```

## `NotPrimitive`

```dream
NotPrimitive
```

## `Int`

```dream
Int
```

## `Long`

```dream
Long
```

## `UInt`

```dream
UInt
```

## `ULong`

```dream
ULong
```

## `ISize`

```dream
ISize
```

## `USize`

```dream
USize
```

## `Byte`

```dream
Byte
```

## `Float`

```dream
Float
```

## `Double`

```dream
Double
```

## `Bool`

```dream
Bool
```

## `Char`

```dream
Char
```

## `String`

```dream
String
```

## `Object`

```dream
Object
```

## `enum CollectionKind`

Standard collection shapes a generator usually special-cases.

```dream
public enum CollectionKind
```

## `NotCollection`

```dream
NotCollection
```

## `List`

```dream
List
```

## `Set`

```dream
Set
```

## `Map`

```dream
Map
```

## `SortedMap`

```dream
SortedMap
```

## `Array`

```dream
Array
```

## `enum TypeRefKind`

What a `GenTypeRef` refers to.

```dream
public enum TypeRefKind
```

## `Primitive`

```dream
Primitive
```
