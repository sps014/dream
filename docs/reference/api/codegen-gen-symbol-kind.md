# GenSymbolKind

**Import:** `import system.codegen;`

## `GenSymbolKind`

The category of a symbol in a generator snapshot.

```dream
public enum GenSymbolKind
```

## `Unknown`

No recognized symbol category was supplied by the snapshot.

```dream
Unknown
```

## `Class`

A reference type declaration.

```dream
Class
```

## `Struct`

A value type declaration.

```dream
Struct
```

## `Enum`

An enum declaration or enum symbol.

```dream
Enum
```

## `Union`

A discriminated union declaration.

```dream
Union
```

## `Interface`

An interface declaration.

```dream
Interface
```

## `Function`

A function that can be referenced or called.

```dream
Function
```

## `Type`

A type name used in a construction or type expression.

```dream
Type
```

## `Local`

A variable declared inside a function.

```dream
Local
```

## `Parameter`

An argument binding declared by a function.

```dream
Parameter
```

## `Global`

A variable declared at module level.

```dream
Global
```

## `Field`

A stored member of an instance.

```dream
Field
```

## `Method`

A callable member of a type.

```dream
Method
```

## `Variant`

An enum member or discriminated-union choice.

```dream
Variant
```

## `Module`

A namespace containing declarations.

```dream
Module
```
