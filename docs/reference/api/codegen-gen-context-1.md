# GenFieldInfo, GenTypeInfo, GenSyntaxBlock, GenContext

**Import:** `import system.codegen;`

Read the [usage guide](../stdlib/codegen.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](codegen-gen-context.md)

## `class GenFieldInfo`

One field on a snapshotted declaration type.

```dream
public class GenFieldInfo
```

## `name: string`

```dream
public name: string
```

## `type_name: string`

```dream
public type_name: string
```

## `constructor`

```dream
public constructor(name: string, type_name: string)
```

## `class GenTypeInfo`

One declaration type snapshotted for an executed `@generator` body.

```dream
public class GenTypeInfo
```

## `name: string`

```dream
public name: string
```

## `attributes: List<string>`

```dream
public attributes: List<string>
```

## `fields: List<GenFieldInfo>`

```dream
public fields: List<GenFieldInfo>
```

## `constructor`

```dream
public constructor(name: string, attributes: List<string>, fields: List<GenFieldInfo>)
```

## `has_attribute`

```dream
public fun has_attribute(attr: string): bool
```

## `class GenSyntaxBlock`

One `introducer { ... }` syntax-DSL call site, snapshotted for an executed `@generator` body.

```dream
public class GenSyntaxBlock
```

## `id: int`

Opaque site id; pass back into `GenContext.replace`/`GenContext.error` unchanged.

```dream
public id: int
```

## `name: string`

The introducer name (`quote`, `html`, ...).

```dream
public name: string
```

## `body: string`

Raw reconstructed body text (splices appear as `{expr}` placeholders).

```dream
public body: string
```

## `splices: List<string>`

Dream source of each `{ ... }` splice expression, in source order.

```dream
public splices: List<string>
```

## `constructor`

```dream
public constructor(id: int, name: string, body: string, splices: List<string>)
```

## `class GenContext`

Compile-time context handed to an executed `@generator` function body. Loaded from a host snapshot (see `from_snapshot`) and flushed back to the host via `finish()`, which prints the same `GenHost` stdout protocol a sibling `harness.dream` would print by hand.

```dream
public class GenContext
```

## `from_snapshot`

Loads the generator's context from its JSON input file. The generator receives that path as its first program argument.

```dream
public static async fun from_snapshot(path: string, token: Option<CancellationToken> = Option.None): Result<GenContext, string>
```

## `syntax_blocks`

Returns the context's syntax blocks. The returned list is shared with `all_blocks`; check each block's `.name` when selecting a particular notation. Do not rearrange or remove blocks from this shared list while using the context.

```dream
public fun syntax_blocks(name: string): List<GenSyntaxBlock>
```

## `all_blocks`

Every snapshotted call site, regardless of introducer.

```dream
public fun all_blocks(): List<GenSyntaxBlock>
```

## `types_with`

Every snapshotted declaration type carrying attribute `attr`.

```dream
public fun types_with(attr: string): List<GenTypeInfo>
```

## `replace`

Rewrites `block`'s call site to the Dream expression `dream_expr` before type-checking.

```dream
public fun replace(borrow block: GenSyntaxBlock, dream_expr: string): void
```

## `emit_extend`

Queues synthesized `extend Type { ... }` source for the host to merge before type-checking.

```dream
public fun emit_extend(type_name: string, body: string): void
```

## `emit_file`

Queues a synthetic Dream source file for the host to parse and merge.

```dream
public fun emit_file(path: string, source: string): void
```
