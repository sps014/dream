# Attributes

**Import:** `import system.core;`

## `AttributeTarget`

Declaration kinds accepted by an attribute.

```dream
public enum AttributeTarget
```

## `Constructor`

Identifies constructor declarations.

```dream
Constructor
```

## `Destructor`

Identifies destructor declarations.

```dream
Destructor
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

## `Type`

Identifies type declarations.

```dream
Type
```

## `Field`

Identifies field declarations.

```dream
Field
```

## `Function`

Identifies function declarations.

```dream
Function
```

## `Method`

Identifies method declarations.

```dream
Method
```

## `StaticMethod`

Identifies static method declarations.

```dream
StaticMethod
```

## `ExternFunction`

Identifies extern function declarations.

```dream
ExternFunction
```

## `InterfaceMethod`

Identifies interface method declarations.

```dream
InterfaceMethod
```

## `Module`

Identifies module declarations.

```dream
Module
```

## `Parameter`

Identifies parameter declarations.

```dream
Parameter
```

## `DeclarationPath`

A declaration reference supplied as an attribute argument.

```dream
public struct DeclarationPath
```

## `text`

Source text represented by this entry.

```dream
public text: string
```

## `symbol`

Resolved declaration identifier, or an empty string when unresolved.

```dream
public symbol: string
```

## `constructor`

Creates a `DeclarationPath` with `text`, `symbol`.

```dream
public constructor(text: string, symbol: string)
```

## `AttributeAttribute`

Declares a `@name(...)` attribute type. Arguments are `AttributeTarget.*` placements (none = anywhere); fields are the argument schema.

```dream
public struct AttributeAttribute
```

## `targets`

Declaration kinds to which this attribute may be applied.

```dream
public targets: AttributeTarget[]
```

## `RepeatableAttribute`

Allows an `@attribute` type to appear more than once on the same declaration.

```dream
public struct RepeatableAttribute
```
