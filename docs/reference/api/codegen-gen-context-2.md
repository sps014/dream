# GenContext

**Import:** `import system.codegen;`

Read the [usage guide](../stdlib/codegen.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](codegen-gen-context.md)

## `error`

```dream
public fun error(borrow call: GenCallSite, message: string): void
```

## `warning`

```dream
public fun warning(borrow decl: GenDecl, message: string): void
```

## `has_errors`

```dream
public fun has_errors(): bool
```

## `log`

A line shown with `dream -v` / `dream generate --explain`.

```dream
public fun log(message: string): void
```

## `finish`

Writes the result for the compiler; the harness returns this as the exit code.

```dream
public async fun finish(): int
```
