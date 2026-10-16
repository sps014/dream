# GenExpr

**Import:** `import system.codegen;`

## `GenExpr`

An expression that a source generator can inspect.

```dream
public class GenExpr
```

## `id`

Stable identifier of this entry.

```dream
public id: string
```

## `kind`

Category of this entry.

```dream
public kind: ExprKind
```

## `statement_kind`

Category of the containing statement.

```dream
public statement_kind: StmtKind
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

Source text represented by this entry.

```dream
public text: string
```

## `op`

Operator spelling used by the expression.

```dream
public op: string
```

## `literal`

Source spelling of the literal.

```dream
public literal: string
```

## `literal_kind`

Category of the literal, such as a number or string.

```dream
public literal_kind: string
```

## `ty`

Type of this declaration or expression.

```dream
public ty: GenTypeRef
```

## `symbol`

Identifier of the referenced symbol.

```dream
public symbol: string
```

## `symbol_kind`

Category of the referenced symbol.

```dream
public symbol_kind: string
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

Creates this model from snapshot JSON.

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
public fun is_kind(kind: ExprKind): bool
```
