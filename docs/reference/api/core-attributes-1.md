# system.core: Attribute declarations

No import is needed.

```dream
public enum AttributeTarget
```

Declaration kinds accepted by an attribute.

```dream
Constructor
```

```dream
Destructor
```

```dream
Class
```

```dream
Struct
```

```dream
Union
```

```dream
Enum
```

```dream
Interface
```

```dream
Type
```

```dream
Field
```

```dream
Function
```

```dream
Method
```

```dream
StaticMethod
```

```dream
ExternFunction
```

```dream
InterfaceMethod
```

```dream
Module
```

```dream
Parameter
```

```dream
public struct DeclarationPath
```

A declaration reference supplied as an attribute argument.

```dream
public text: string
```

```dream
public constructor(text: string)
```

```dream
public struct AttributeAttribute
```

Declares a `@name(...)` attribute type. Arguments are `AttributeTarget.*` placements (none = anywhere); fields are the argument schema.

```dream
public targets: AttributeTarget[]
```

```dream
public struct RepeatableAttribute
```

Allows an `@attribute` type to appear more than once on the same declaration.

```dream
public struct ExportAttribute
```

Exports a non-generic synchronous function through a plain C ABI wrapper.
