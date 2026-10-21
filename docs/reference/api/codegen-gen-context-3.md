# Gen Context

**Import:** `import system.codegen;`

## `error`

Reports an error at the field; generation fails after the generator exits.

```dream
public fun error(borrow field: GenField, message: string): void
```

## `error`

Reports an error at the call site; generation fails after the generator exits.

```dream
public fun error(borrow call: GenCallSite, message: string): void
```

## `warning`

Reports a warning at the declaration without failing generation.

```dream
public fun warning(borrow decl: GenDecl, message: string): void
```

## `has_errors`

Whether this generator has reported any error so far.

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
