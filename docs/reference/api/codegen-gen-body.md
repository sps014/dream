# GenBody

**Import:** `import system.codegen;`

## `GenBody`

Statements belonging to a function body, in source order.

```dream
public class GenBody
```

## `statements`

Statements in source order.

```dream
public statements: List<GenStmt>
```

## `constructor`

Decodes a `GenBody` from its object in the generation snapshot.

```dream
public constructor(borrow v: JsonValue)
```
