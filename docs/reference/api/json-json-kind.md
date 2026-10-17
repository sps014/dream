# JsonKind

**Import:** `import system.json;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `JsonKind`

Discriminant for `JsonValue.kind()`.

```dream
public enum JsonKind
```

## `Null`

A JSON null value.

```dream
Null
```

## `Bool`

A JSON boolean value.

```dream
Bool
```

## `Number`

A JSON numeric value.

```dream
Number
```

## `String`

A JSON string value.

```dream
String
```

## `Array`

An ordered JSON array of values.

```dream
Array
```

## `Object`

A JSON object mapping property names to values.

```dream
Object
```
