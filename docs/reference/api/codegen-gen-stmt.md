# GenStmt

**Import:** `import system.codegen;`

## `public class GenStmt`

```dream
public class GenStmt
```

## `public id: string`

```dream
public id: string
```

## `public kind: StmtKind`

```dream
public kind: StmtKind
```

## `public span: GenSpan`

```dream
public span: GenSpan
```

## `public constant: Option<GenConstant>`

```dream
public constant: Option<GenConstant>
```

## `public text: string`

```dream
public text: string
```

## `public ty: GenTypeRef`

```dream
public ty: GenTypeRef
```

## `public symbol: string`

```dream
public symbol: string
```

## `public overload: string`

```dream
public overload: string
```

## `public type_args: List<GenTypeRef>`

```dream
public type_args: List<GenTypeRef>
```

## `public types: List<GenTypeRef>`

```dream
public types: List<GenTypeRef>
```

## `public constructor(borrow v: JsonValue)`

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
public fun is_kind(kind: StmtKind): bool
```
