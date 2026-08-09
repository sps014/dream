# `system.codegen`

Helpers for compile-time source generators.

```dream
import system.codegen;
```

Syntax DSLs are **not** part of this package — see
[Source generators](../language/generators.md). Beginner sample:
[`sample/generators/quote/`](https://github.com/sps014/dream/tree/main/sample/generators/quote).
Advanced (markup parser + harness):
[`sample/generators/html/`](https://github.com/sps014/dream/tree/main/sample/generators/html).


## Status

| Piece | Today |
|-------|--------|
| `CodeBuilder` | Shipped — build Dream source strings |
| `GenHost` | Shipped — OK/ERR/LOC stdout markers for harnesses |
| `GenResult` (`system.json`) | Shipped — expand outcome + optional type/field for spans |
| `GenContext` / `GenSyntaxBlock` | Shipped — Dream-side context for an *executed* `@generator` body: `syntax_blocks`, `replace`, `error`, `finish` |
| Host `GeneratorContext` | Rust only (`driver/generate`) — `emit_*`, `replace`, `error` |
| User `@generator` Dream bodies | **Executed** when the function takes a single `GenContext` and has a non-empty body (see below); an empty body still falls back to a sibling `harness.dream` |
| Builtin `@json` | Shipped in the driver (language derive) |
| Syntax-DSL harness runner | Shipped — generic snapshot → harness WASM → `replace` (used for both the auto-generated executed-body harness and a hand-written sibling `harness.dream`) |

When a Dream harness runs (as `@json` does), print `GenHost.err_marker()` then the message, and optionally `GenHost.loc_marker()` + `GenHost.format_loc(type, field)` so the host can attach a source span via `DiagnosticBag`. Failures surface as `CompileError::Generator`.

## `CodeBuilder`

Accumulates Dream source for `emit_extend` / `emit_file` bodies. Construct with `CodeBuilder()`
(4 spaces per indent level) or `CodeBuilder.with_spaces(n)` for `n` spaces per level.

#### `CodeBuilder()` / `CodeBuilder.with_spaces(n)`

Creates an empty builder with default or custom indent width. Use `with_spaces` when generated code must match a project's style guide.

```dream
let b = CodeBuilder();
let tight = CodeBuilder.with_spaces(2);
```

#### `indent()` / `dedent()` / `line(text)` / `append(text)` / `to_string()`

Builds source line by line: `line` adds a newline and applies indent at line start; `append` adds inline text without re-indenting mid-line.

```dream
let b = CodeBuilder();
b.line("public fun to_json(): JsonValue {");
b.indent();
b.line("return JsonValue.dict();");
b.dedent();
b.line("}");
b.append("// trailing comment");
let src = b.to_string();
```

Indent is applied only at the start of a line. Mid-line `append` does not re-prefix.

## `GenHost`

#### `ok_marker()` / `err_marker()` / `loc_marker()` / `format_loc(type, field)`

Returns the stdout marker strings the compile host expects from generator harnesses. Print `err_marker()` + message on failure; optionally `loc_marker()` + `format_loc` to point at a type field.

```dream
System.println(GenHost.ok_marker());
System.println(GenHost.err_marker());
System.println(GenHost.loc_marker());
System.println(GenHost.format_loc("User", "name"));
```

## `GenContext` / `GenSyntaxBlock`

Compile-time context handed to an `@generator(ctx: GenContext)` body when it claims a
`@syntax_block`. See [Source generators](../language/generators.md#your-first-custom-generator-quote)
for the end-to-end walkthrough.

#### `GenContext.from_snapshot(path)`

Loads a `GenContext` from the JSON snapshot the host wrote (usually
`System.env_or("DREAM_SYNTAX_GEN_SNAPSHOT", "")`). The compiler's auto-generated harness calls this
for you; only call it directly if you're hand-rolling a harness that wants `GenContext`'s parsing.

#### `syntax_blocks(name)` / `all_blocks()`

Returns every `GenSyntaxBlock` snapshot matching introducer `name` (or every site, for
`all_blocks()`). Each `GenSyntaxBlock` has `id`, `name`, `body` (raw text, splices as `{expr}`
placeholders), and `splices` (`List<string>` of each splice's Dream source, in order).

#### `replace(block, dream_expr)` / `error(block, message)` / `error_general(message)`

Queues a rewrite or a generate-time diagnostic for later flushing; `error`/`error_general` keep
only the first reported failure, matching the `GenHost` `ERR` protocol's one-message contract.

#### `finish()`

Flushes every queued `replace`/`error` call to stdout using the `GenHost` protocol. The
auto-generated harness calls this once after your `@generator` function returns.

```dream
import system.codegen;

@generator
@syntax_block("quote")
public fun quote(ctx: GenContext): void {
    for (let block in ctx.syntax_blocks("quote")) {
        ctx.replace(block, "\"" + block.body.trim() + "\"");
    }
}
```

## `GenResult` (`system.json`)

Outcome of a generator expand step.

#### `GenResult.success(source)` / `failure(error)` / `failure_at(error, type, field)`

Constructs the outcome object a harness returns: success with generated source, plain failure, or failure tied to a type/field for span attachment.

```dream
import system.json;

let ok = GenResult.success("extend User { }");
let bad = GenResult.failure("unsupported field");
let at = GenResult.failure_at("bad type", "User", "age");
System.println(ok.ok);
System.println(bad.error);
```

Fields: `ok`, `source`, `error`, `error_type`, `error_field`.

## Generator host API (Rust)

Generators discover work via `@generator` functions / attributes. The compile host exposes:

### Discovery

- `ctx.types()` / `ctx.types_with("attr")` → type symbols
- `ctx.functions_with("attr")` → function symbols
- `ctx.syntax_blocks("introducer")` → sites for `introducer { … }`

### Emit / replace / errors

- `ctx.emit_extend(type_name, body)` — synthesize `extend Type { body }`
- `ctx.emit_file(path, source)` — parse a synthetic Dream file
- `ctx.replace(node, dream_expr)` — rewrite a syntax-DSL site
- `ctx.error(node, message)` — queue a generate-time diagnostic (flushed into `DiagnosticBag`)

### Shipped generators

| Feature | Where |
|---------|--------|
| `@json` derive | [JSON](json.md) — compiler builtin |
| Quote sample | [`sample/generators/quote/`](https://github.com/sps014/dream/tree/main/sample/generators/quote) |
| HTML sample | [`sample/generators/html/`](https://github.com/sps014/dream/tree/main/sample/generators/html) |

Full tutorial: [Source generators](../language/generators.md).
