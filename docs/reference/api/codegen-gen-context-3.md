# Gen Context

**Import:** `import system.codegen;`

## `error`

Reports an error at this model node or at the generator when no node is supplied.

```dream
public fun error(borrow field: GenField, message: string): void
```

## `error`

Reports an error at this model node or at the generator when no node is supplied.

```dream
public fun error(borrow call: GenCallSite, message: string): void
```

## `warning`

Reports a warning at this model node.

```dream
public fun warning(borrow decl: GenDecl, message: string): void
```

## `has_errors`

Returns the requested model information.

```dream
public fun has_errors(): bool
```

## `log`

A line shown with `dream -v` / `dream generate --explain`.

```dream
public fun log(message: string): void
```

## `finish`

Writes the generator result and returns its exit status.

```dream
public async fun finish(): int
```
