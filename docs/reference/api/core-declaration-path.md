# DeclarationPath

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `DeclarationPath`

A declaration reference supplied as an attribute argument.

```dream
public struct DeclarationPath
```

## `text`

Source text represented by this entry.

```dream
public text: string
```

## `symbol`

Resolved declaration identifier, or an empty string when unresolved.

```dream
public symbol: string
```

## `constructor`

Creates a `DeclarationPath` with `text`, `symbol`.

```dream
public constructor(text: string, symbol: string)
```
