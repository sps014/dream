# GenFieldInfo, GenTypeInfo, GenSyntaxBlock, GenContext

**Import:** `import system.codegen;`

Read the [usage guide](../stdlib/codegen.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](codegen-gen-context.md)

## `error`

Queues a generate-time diagnostic for `block` (surfaces as `CompileError::Generator`). Only the first reported error is kept, mirroring the harness `ERR` protocol's one-message contract.

```dream
public fun error(borrow block: GenSyntaxBlock, message: string): void
```

## `error_general`

Queues a generate-time diagnostic with no associated call site.

```dream
public fun error_general(message: string): void
```

## `finish`

Flushes accumulated replace/error/emit calls to stdout using the `GenHost` protocol.

```dream
public fun finish(): void
```
