# GenConstant

**Import:** `import system.codegen;`

## `GenConstant`

A constant value and its kind, as seen by a source generator.

```dream
public class GenConstant
```

## `kind`

Category of this entry.

```dream
public kind: string
```

## `value`

Value carried by this entry.

```dream
public value: string
```

## `constructor`

Decodes an exact compile-time scalar value; absence means no safe constant was inferred.

```dream
public constructor(borrow v: JsonValue)
```
