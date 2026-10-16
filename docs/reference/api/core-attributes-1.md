# Attributes

**Import:** `import system.core;`

## `public enum AttributeTarget`

Declaration kinds accepted by an attribute.

```dream
public enum AttributeTarget
```

## `Constructor`

```dream
Constructor
```

## `Destructor`

```dream
Destructor
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

## `Type`

```dream
Type
```

## `Field`

```dream
Field
```

## `Function`

```dream
Function
```

## `Method`

```dream
Method
```

## `StaticMethod`

```dream
StaticMethod
```

## `ExternFunction`

```dream
ExternFunction
```

## `InterfaceMethod`

```dream
InterfaceMethod
```

## `Module`

```dream
Module
```

## `Parameter`

```dream
Parameter
```

## `public struct DeclarationPath`

A declaration reference supplied as an attribute argument.

```dream
public struct DeclarationPath
```

## `public text: string`

```dream
public text: string
```

## `public symbol: string`

Canonical compiler identity; empty when the reference could not be resolved.

```dream
public symbol: string
```

## `public constructor`

```dream
public constructor(text: string, symbol: string)
```

## `public struct AttributeAttribute`

Declares a `@name(...)` attribute type. Arguments are `AttributeTarget.*` placements (none = anywhere); fields are the argument schema.

```dream
public struct AttributeAttribute
```

## `public targets: AttributeTarget[]`

```dream
public targets: AttributeTarget[]
```

## `public struct RepeatableAttribute`

Allows an `@attribute` type to appear more than once on the same declaration.

```dream
public struct RepeatableAttribute
```
