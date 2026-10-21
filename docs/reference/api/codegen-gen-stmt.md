# GenStmt

**Import:** `import system.codegen;`

## `GenStmt`

A statement that a source generator can inspect.

```dream
public class GenStmt
```

## `id`

Identity of this model node; pass it to `GenContext.error_at` to report a diagnostic here.

```dream
public id: string
```

## `kind`

Syntactic shape of the statement (`let`, `return`, `if`, ...).

```dream
public kind: StmtKind
```

## `span`

Source range occupied by this node.

```dream
public span: GenSpan
```

## `constant`

Constant value of the expression, when known.

```dream
public constant: Option<GenConstant>
```

## `text`

Source text of the statement as written.

```dream
public text: string
```

## `ty`

Type of the value the statement produces, or Unknown when it has none.

```dream
public ty: GenTypeRef
```

## `symbol`

Identifier of the referenced symbol.

```dream
public symbol: string
```

## `overload`

Identifier distinguishing this function's overload.

```dream
public overload: string
```

## `type_args`

Concrete type arguments, in parameter order.

```dream
public type_args: List<GenTypeRef>
```

## `types`

Type references associated with this expression.

```dream
public types: List<GenTypeRef>
```

## `constructor`

Decodes a `GenStmt` from its object in the generation snapshot.

```dream
public constructor(borrow v: JsonValue)
```

## `children`

Returns child nodes in source order.

```dream
public fun children(): List<GenExpr>
```

## `is_kind`

Tests the node kind.

```dream
public fun is_kind(kind: StmtKind): bool
```
