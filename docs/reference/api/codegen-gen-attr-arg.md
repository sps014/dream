# Gen Attr Arg

**Import:** `import system.codegen;`

## `GenAttrArg`

One attribute argument as written: `kind` is `string`, `int`, `float`, `double`, `bool` or `enum`; `value` is the unquoted string, literal text, or dotted enum path.

```dream
public class GenAttrArg
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

## `symbol`

Resolved identifier of a declaration argument, or an empty string when unresolved.

```dream
public symbol: string
```

## `constructor`

Creates the snapshot model from the supplied values.

```dream
public constructor(kind: string, value: string, symbol: string)
```
