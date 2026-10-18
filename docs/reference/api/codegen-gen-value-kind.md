# GenValueKind

**Import:** `import system.codegen;`

## `GenValueKind`

The category of a literal, constant, or attribute argument.

```dream
public enum GenValueKind
```

## `Unknown`

No recognized value category was supplied by the snapshot.

```dream
Unknown
```

## `String`

A string value, with its unquoted contents in the value field.

```dream
String
```

## `Int`

A signed integer constant represented as decimal text.

```dream
Int
```

## `UInt`

An unsigned integer constant represented as decimal text.

```dream
UInt
```

## `Float`

A single-precision floating-point value.

```dream
Float
```

## `Double`

A double-precision floating-point value.

```dream
Double
```

## `Bool`

A Boolean value represented as true or false.

```dream
Bool
```

## `Char`

A character literal.

```dream
Char
```

## `Long`

A signed 64-bit integer literal.

```dream
Long
```

## `ULong`

An unsigned 64-bit integer literal.

```dream
ULong
```

## `Float16`

A half-precision floating-point literal.

```dream
Float16
```

## `Enum`

An enum member represented by its qualified source name.

```dream
Enum
```
