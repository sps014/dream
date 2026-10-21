# Gen Attr Arg

**Import:** `import system.codegen;`

## `GenAttrArg`

One attribute argument as written: `kind` is `string`, `int`, `float`, `double`, `bool` or `enum`; `value` is the unquoted string, literal text, or dotted enum path.

```dream
public class GenAttrArg
```

## `kind`

Literal category of the argument as written (`string`, `int`, `enum`, ...).

```dream
public kind: GenValueKind
```

## `value`

Unquoted string, literal text, or dotted enum path, depending on `kind`.

```dream
public value: string
```

## `symbol`

Resolved identifier of a declaration argument, or an empty string when unresolved.

```dream
public symbol: string
```

## `constructor`

Creates a `GenAttrArg` from its parts. Generators normally receive these from `GenContext` rather than constructing them.

```dream
public constructor(kind: GenValueKind, value: string, symbol: string)
```
