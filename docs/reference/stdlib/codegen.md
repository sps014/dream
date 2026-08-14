# `system.codegen`

Helpers for **compile-time source generators** you write in Dream.

```dream
import system.codegen;
```

Tutorial and samples: [Source generators](../language/generators.md),
[`sample/generators/quote/`](https://github.com/sps014/dream/tree/main/sample/generators/quote),
[`sample/generators/html/`](https://github.com/sps014/dream/tree/main/sample/generators/html),
[`sample/generators/dto/`](https://github.com/sps014/dream/tree/main/sample/generators/dto).

## `CodeBuilder`

Builds indented Dream source strings — use in parser/emit helpers that return generated code.

Construct with `CodeBuilder()` (4 spaces per indent) or `CodeBuilder.with_spaces(n)`.

```dream
let b = CodeBuilder();
b.line("public fun to_json(): JsonValue {");
b.indent();
b.line("return JsonValue.dict();");
b.dedent();
b.line("}");
let src = b.to_string();
```

| Method | Effect |
|--------|--------|
| `indent()` / `dedent()` | Adjust indent depth |
| `line(text)` | New line at current indent |
| `append(text)` | Inline text (indent only at line start) |
| `to_string()` | Final source text |

## `GenFieldInfo`

One field on a snapshotted declaration type.

| Field | Type | Meaning |
|-------|------|---------|
| `name` | `string` | Field name |
| `type_name` | `string` | Display type (e.g. `int`, `string`) |

## `GenTypeInfo`

One declaration type in the generator snapshot (classes, structs, unions, …).

| Field | Type | Meaning |
|-------|------|---------|
| `name` | `string` | Type name |
| `attributes` | `List<string>` | Attribute names on the declaration |
| `fields` | `List<GenFieldInfo>` | Instance fields (empty for plain enums) |

`has_attribute(attr: string): bool` — whether the type carries attribute `attr`.

## `GenSyntaxBlock`

One snapshotted `introducer { … }` call site, passed to your `@generator` body.

| Field | Type | Meaning |
|-------|------|---------|
| `id` | `int` | Opaque site id — pass back to `replace` / `error` unchanged |
| `name` | `string` | Introducer (`quote`, `html`, …) |
| `body` | `string` | Raw body text; splices appear as `{expr}` placeholders |
| `splices` | `List<string>` | Dream source of each splice, in order |

## `GenContext`

Handed to an executed `@generator(ctx: GenContext)` function. The compiler loads it from a
snapshot of every syntax-block site and declaration type, runs your function, then applies
queued replaces, emits, and errors.

Write your generator against these methods:

#### `syntax_blocks(name: string): List<GenSyntaxBlock>`

Every site for introducer `name` (e.g. `ctx.syntax_blocks("quote")`).

#### `all_blocks(): List<GenSyntaxBlock>`

Every snapshotted site, regardless of introducer.

#### `types_with(attr: string): List<GenTypeInfo>`

Every snapshotted type carrying attribute `attr` (e.g. `ctx.types_with("dto")`).

#### `replace(block: GenSyntaxBlock, dream_expr: string): void`

Rewrites `block`'s call site to `dream_expr` before type-checking. Queue one line per site.

#### `emit_extend(type_name: string, body: string): void`

Queues synthesized `extend Type { ... }` member source. `body` is the inner block (methods only).

#### `emit_file(path: string, source: string): void`

Queues a synthetic Dream source file for the host to parse and merge.

#### `error(block: GenSyntaxBlock, message: string): void`

Reports a generate-time diagnostic tied to a site. Only the first error is kept.

#### `error_general(message: string): void`

Reports a generate-time error with no associated site.

```dream
@generator
@syntax_block("quote")
public fun quote(ctx: GenContext): void {
    for (let block in ctx.syntax_blocks("quote")) {
        ctx.replace(block, "\"" + block.body.trim() + "\"");
    }
}
```

Failures surface as `CompileError::Generator` in the main compile diagnostic stream.

## `GenResult` (`system.json`)

Outcome type for compile helpers (e.g. a markup parser). Import `system.json` alongside
`system.codegen`.

```dream
import system.json;

let ok = GenResult.success("Html.render(...)");
let bad = GenResult.failure("unexpected token");
let at = GenResult.failure_at("bad field", "User", "age");
```

Fields: `ok`, `source`, `error`, `error_type`, `error_field`. On failure, call
`ctx.error(block, result.error)` from your generator.

## Related

| Feature | Where |
|---------|--------|
| `@json` derive | [JSON](json.md) — compiler builtin, not `GenContext` |
| Quote sample | [`sample/generators/quote/`](https://github.com/sps014/dream/tree/main/sample/generators/quote) |
| HTML sample | [`sample/generators/html/`](https://github.com/sps014/dream/tree/main/sample/generators/html) |
| DTO emit sample | [`sample/generators/dto/`](https://github.com/sps014/dream/tree/main/sample/generators/dto) |
