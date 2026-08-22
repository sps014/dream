# Source generators

Dream has **no runtime reflection**. When you need a DSL, compile-time rewriting, or boilerplate
that depends on types, write a **compile-time source generator**.

Generators run during compilation — before type-checking on the final program — and do one of two
things:

1. **Replace** custom syntax — `quote { … }` or `html { … }` become ordinary Dream expressions at
   call sites. Use this when the braced body is a domain-specific shape the main parser does not
   understand.
2. **Emit** new Dream source — for example `@json` adds `to_json` / `from_json` on your types.
   Use this when call sites should stay ordinary Dream and generated code is methods or helpers.

This page focuses on **writing replace-style generators** with `@generator`, `GenContext`, and
`@syntax_block`. API details live in [`system.codegen`](../stdlib/codegen.md). For the shipped
JSON derive, see [JSON](../stdlib/json.md) (`@json` is a compiler builtin — you do not register it).

## Your first generator: `quote`

`quote { … }` turns the text inside the braces into a Dream string literal at compile time.

```dream
import system;

fun main() {
    System.println(quote { Hello generators });
}
```

```bash
dream run sample/generators/quote/app.dream
```

Expected stdout: `Hello generators`

Full sample: [`sample/generators/quote/`](https://github.com/sps014/dream/tree/main/sample/generators/quote).

### Register the generator

Tell the compiler which file owns the generator:

- **`dream.toml`** — list the generator next to your entry file (search walks upward from the
  entry file's directory).
- **`import`** — import a module that contains the `@generator` function.

Use `dream.toml` when the generator should always load with the project; use an import when apps
opt in by depending on a module.

```toml
[[generators]]
path = "gen.dream"
```

```dream
module gen;

import system.codegen;

@generator
@syntax_block("quote")
public fun quote(ctx: GenContext): void {
    for (let block in ctx.syntax_blocks("quote")) {
        ctx.replace(block, as_dream_string(block.body.trim()));
    }
}

fun as_dream_string(s: string): string {
    let escaped = s.replace("\\", "\\\\").replace("\"", "\\\"");
    return "\"" + escaped + "\"";
}
```

A `@generator` function with a single `GenContext` parameter and a non-empty body is **compiled and
run by the compiler**. It receives every matching syntax-block site, queues rewrites with
`ctx.replace`, and reports failures with `ctx.error` — all before type-checking.

| Attribute | Where | Meaning |
|-----------|--------|---------|
| `@generator` | function | Generator entry; the function name is its identity |
| `@syntax_block("intro")` | same function | Claims expression sites `intro { … }` |

Without a registered `@syntax_block` for an introducer, call sites fail with “unexpanded syntax
block”.

### `GenContext` workflow

For each `introducer { … }` site the compiler snapshots:

- **`body`** — opaque text inside the braces (your DSL grammar).
- **`splices`** — Dream source of each `{expr}` splice, in order (splices are real expressions and
  type-check after rewrite).

Your generator walks matching blocks and either replaces or errors:

```dream
for (let block in ctx.syntax_blocks("quote")) {
    ctx.replace(block, "\"literal\"");
    // ctx.error(block, "message");  // first error wins; becomes CompileError::Generator
}
```

See [`GenContext` / `GenSyntaxBlock`](../stdlib/codegen.md#gensyntaxblock) for the full API.

### Syntax DSL rules

- The introducer is a bare identifier (`quote`, `html`, …) — not a keyword. Pick a name that will
  not collide with identifiers in user scope.
- Text inside `{ … }` is opaque to the Dream parser until your generator rewrites the site.
- `{ … }` splices must be valid Dream expressions; they are type-checked after rewrite.
- Errors inside a replaced site — whether the generated text fails to parse or does not
  type-check — are reported against the original `introducer { … }` block in your source file,
  not the replacement text.
- Every introducer must be claimed by `@syntax_block("…")` on a registered `@generator`.

## A larger example: HTML

Same call-site shape as `quote`, but the sample parses markup and lowers it to runtime helpers.

```dream
import system;
import html;

fun page(title: string): string {
    return html {
        <div class="hero">
            <h1>{title}</h1>
            <p>Welcome</p>
        </div>
    };
}

fun main() {
    System.println(page("Hello"));
}
```

```bash
dream run sample/generators/html/app.dream
```

HTML is **not** a language builtin. The sample's `gen.dream` calls `HtmlCompiler` (in
`parser.dream`) from its `GenContext` body; complexity stays in helper modules, not in the
one-line call site.

Full sample: [`sample/generators/html/`](https://github.com/sps014/dream/tree/main/sample/generators/html).

Typical layout:

| File | Role |
|------|------|
| `app.dream` | Program that uses `intro { … }` |
| `gen.dream` | `@generator` + `@syntax_block`, `GenContext` replace logic |
| `parser.dream` (optional) | DSL → Dream source (returns `GenResult` on failure) |
| `dream.toml` | `[[generators]] path = "gen.dream"` |

## Emit-style derives (`@json` and custom attributes)

For method synthesis on existing types, the shipped example is **`@json`**: mark a class or union
and the compiler generates converters — no custom `@generator` registration. See
[JSON](../stdlib/json.md).

Custom emit generators use `GenContext.types_with` and `emit_extend` / `emit_file` to queue
synthesized Dream source before type-checking. Declare the attribute schema with `@attribute`,
then register a `@generator` body that reads matching types from the snapshot:

```dream
@attribute
public fun dto(): void { }

@generator
public fun dto_derive(ctx: GenContext): void {
    for (let t in ctx.types_with("dto")) {
        let b = CodeBuilder();
        b.line("public fun describe(): string {");
        b.indent();
        b.line("return \"dto\";");
        b.dedent();
        b.line("}");
        ctx.emit_extend(t.name, b.to_string());
    }
}
```

```dream
import gen;

@dto
class Point {
    public x: int;
    public y: int;
}
```

Full sample: [`sample/generators/dto/`](https://github.com/sps014/dream/tree/main/sample/generators/dto).

A `@generator` with a `GenContext` body runs even when it claims no `@syntax_block` introducers —
the snapshot still includes every declaration type for `types_with`.

## Building generated source with `CodeBuilder`

When a helper module emits multi-line Dream source (as `HtmlCompiler` does), use
[`CodeBuilder`](../stdlib/codegen.md#codebuilder) instead of manual indent strings:

```dream
import system.codegen;

let b = CodeBuilder();
b.line("public fun describe(): string {");
b.indent();
b.line("return \"ok\";");
b.dedent();
b.line("}");
let body = b.to_string();
```

Pair with [`GenResult`](../stdlib/json.md#genresult) (`system.json`) to return success/failure
from compile helpers.

## User-defined attributes

Declare custom attribute schemas so user code can annotate declarations. The function name is the
attribute name; parameters define `@name(arg, …)` arguments.

```dream
@attribute
public fun route(path: string): void { }

@route("/users")
public fun list_users(): void { }
```

Syntax DSL generators usually read `block.body` and splices rather than declaration attributes.
For derive-style metadata on types, use `@json` or the field options documented in
[JSON](../stdlib/json.md).

## Checklist

1. Decide **replace** (DSL) vs **emit** (derive). This guide covers both: replace via
   `syntax_blocks` / `replace`, emit via `types_with` / `emit_extend` / `emit_file`.
2. Add `@generator` and, for DSL generators, `@syntax_block("intro")` on a function
   `intro(ctx: GenContext): void`.
3. Implement `ctx.syntax_blocks("intro")` → `ctx.replace` / `ctx.error`, or
   `ctx.types_with("attr")` → `ctx.emit_extend` / `ctx.emit_file`.
4. Register via `[[generators]]` in `dream.toml` or an `import` of the generator module.
5. Use `CodeBuilder` and `GenResult` for multi-line emit helpers when the DSL needs a parser.
6. Add a sample under `sample/generators/` or a golden test under `tests/cases/`.

## See also

- [`system.codegen`](../stdlib/codegen.md) — `CodeBuilder`, `GenContext`, `GenSyntaxBlock`
- [JSON](../stdlib/json.md) — `@json` derive
- [`sample/generators/quote/`](https://github.com/sps014/dream/tree/main/sample/generators/quote)
- [`sample/generators/html/`](https://github.com/sps014/dream/tree/main/sample/generators/html)
- [`sample/generators/dto/`](https://github.com/sps014/dream/tree/main/sample/generators/dto)
