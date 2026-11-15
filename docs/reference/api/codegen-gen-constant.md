# GenConstant

**Import:** `import system.codegen;`

## `GenConstant`

A constant value and its kind, as seen by a source generator.

```dream
public class GenConstant
```

## `kind`

Scalar category of the constant (`int`, `string`, `bool`, ...).

```dream
public kind: GenValueKind
```

## `value`

The constant's value as literal text; strings are unquoted.

```dream
public value: string
```

## `constructor`

Decodes an exact compile-time scalar value; absence means no safe constant was inferred.

```dream
public constructor(borrow v: JsonValue)
```
