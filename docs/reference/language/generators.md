# Source generators

A source generator writes or replaces Dream code while your program is being built. Use it for repeated code or a small custom notation. Dream does not inspect arbitrary type definitions at runtime.

A generator is an ordinary Dream function compiled into a small cached executable. It runs before type-checking and does one of two things:

1. **Replace** custom syntax — `quote { … }` or `html { … }` become ordinary Dream expressions at call sites. Use this when the braced body is a domain-specific shape the main parser does not understand.
2. **Emit** new Dream source — for example `@json` adds `to_json` / `from_json` to your types. Use this when call sites stay ordinary Dream and the generated code is methods or helpers.

API details live in `[system.codegen](../stdlib/codegen.md)`. The shipped `@json` derive is itself a Dream generator in `system.json`; see [JSON](../stdlib/json.md).

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

Full sample: `[sample/generators/quote/](https://github.com/sps014/dream/tree/main/sample/generators/quote)`.

### Register the generator

Tell the compiler which file owns the generator:

- `**dream.toml**` — list the generator next to your entry file (search walks upward from the entry file's directory).
- `**import**` — import a module that contains the `@generator` function. This is how the standard library ships `@json`: `import system.json;` makes it available in single files, packages, dependencies, `dream test`, and wasm builds alike.

```toml
[[generators]]
path = "gen.dream"
```

```dream
module gen;

import system.codegen;

// `quote { ... }` becomes a Dream string literal at compile time.
@generator
@syntax_block
public fun quote(ctx: GenContext): void {
    for (let block in ctx.syntax_blocks()) {
        ctx.replace(block, as_dream_string(block.body.trim()));
    }
}

fun as_dream_string(s: string): string {
    let escaped = s.replace("\\", "\\\\").replace("\"", "\\\"");
    return "\"" + escaped + "\"";
}
```

The function takes a single `GenContext`. It queues rewrites with `ctx.replace` and reports failures with `ctx.error`. The compiler validates and applies them before type-checking.

## Triggers

A generator runs only when the program contains something it declares interest in. If no trigger matches, the generator is skipped without building or launching anything, so importing a generator package costs nothing for programs that don't use it.


| Attribute          | Runs when                                              | Context API                                                                 |
| ------------------ | ------------------------------------------------------ | --------------------------------------------------------------------------- |
| `@syntax_block`    | a `name { … }` site uses the generator's function name | `ctx.syntax_blocks()`                                                       |
| `@on_attribute(A)` | a declaration (or one of its members) carries `@A`     | `ctx.types_with<A>()`, `ctx.functions_with<A>()`, `ctx.decls()`             |
| `@on_call(f, …)`   | the program calls one of the named functions           | `ctx.call_sites()`                                                          |
| `@incremental`     | (modifier)                                             | replays the cached result when the generator and its snapshot are unchanged |


Triggers are paths, not strings. `@on_attribute(jsn)` is a compile error (`'jsn' is not a declared @attribute type`), and so is naming something that is not an `@attribute` struct. A generator may combine several triggers; `@json` uses `@on_attribute(json)` and `@on_call(Json.serialize, Json.deserialize, Json.from_value)`.

Without a matching `@syntax_block` generator, a `name { … }` site fails with “unexpanded syntax block”.

### Syntax DSL rules

For each `name { … }` site, `GenSyntaxBlock` provides:

- `**body**` — opaque text inside the braces (your DSL grammar).
- `**splices**` — Dream source of each `{expr}` splice, in order. Splices are real expressions and type-check after the rewrite.

Rules:

- The introducer is a bare identifier (`quote`, `html`, …), not a keyword. Pick a name that will not collide with identifiers in user scope.
- Text inside `{ … }` is opaque to the Dream parser until your generator rewrites the site.
- Errors inside a replaced site — whether the generated text fails to parse or does not type-check — are reported against the original `name { … }` block, not the replacement text.

## A larger example: HTML

Same call-site shape as `quote`, but the sample parses markup and turns it into ordinary Dream calls.

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

HTML is **not** a language builtin. The sample's `gen.dream` calls `HtmlCompiler` (in `parser.dream`) from its generator body, so the complexity stays in helper modules.

Full sample: `[sample/generators/html/](https://github.com/sps014/dream/tree/main/sample/generators/html)`.


| File           | Role                                                          |
| -------------- | ------------------------------------------------------------- |
| `app.dream`    | Program that uses `html { … }`                                |
| `gen.dream`    | `@generator @syntax_block` replace logic                      |
| `parser.dream` | DSL → Dream source, with its own success/failure result class |
| `dream.toml`   | `[[generators]] path = "gen.dream"`                           |


## Attributes

Declare an attribute as a struct marked `@attribute(AttributeTarget.X)`. The struct's fields are the attribute's arguments, and the target says where it may appear (`Class`, `Struct`, `Union`, `Function`, `Field`, …). Applying it elsewhere is a compile error such as `'@route' cannot be applied to a class`.

```dream
@attribute(AttributeTarget.Function)
public struct route {
    public path: string;
}

@route("/users")
public fun list_users(): void { }
```

Inside a generator, read attributes as typed values: `decl.has<route>()`, `decl.attribute<route>()` (an `Option<route>` with `path` filled in), and `attributes_of<A>()` for repeated attributes.

## Emit-style derives

Emit generators read declarations from the context and queue new source with `ctx.emit_extend(decl, body)` (adds members to an existing type) or `ctx.emit_file(path, source)` (adds a whole file).

```dream
module gen;

import system.codegen;

@attribute(AttributeTarget.Class)
public struct dto {}

@generator
@on_attribute(dto)
public fun dto_derive(ctx: GenContext): void {
    for (let t in ctx.types_with<dto>()) {
        let b = CodeBuilder();
        b.line("public fun describe(): string {");
        b.indent();
        b.line("return \"" + t.name + " is a dto\";");
        b.dedent();
        b.line("}");
        ctx.emit_extend(t, b.to_string());
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

Full sample: `[sample/generators/dto/](https://github.com/sps014/dream/tree/main/sample/generators/dto)`.

The declaration model is typed:

- `GenDecl` carries `kind` (`DeclKind.Class` / `Struct` / `Union` / …), `fields`, `variants`, `methods`, `constructors`, and `generics`.
- `GenField.ty` is a `GenTypeRef`. Use `primitive()` (`Primitive.Int`, …), `collection()` (`CollectionKind.List`, …), `is_option()`, and `arg(i)` instead of comparing type names as strings.
- `ctx.type_index()` lists every type in the program by name and kind, and `ctx.find_type(name)` looks one up.
- `ctx.options<T>()` decodes `dream.toml` `[[generators]].options` into an `@json` type, and `ctx.additional_files()` returns the files listed in `additional_files`.

### `declof`

`declof(path)` evaluates to the identity string of a type, function, field, method, or variant. Use it to compare against model identities, such as `GenCallSite.callee` or `GenTypeRef.is_decl(id)`, without spelling them by hand:

```dream
let serialize = declof(Json.serialize);   // "system.json::Json.serialize"
for (let call in ctx.call_sites()) {
    if call.callee == serialize { /* ... */ }
}
```

A path that resolves to nothing is a compile error.

1. Generated files

Emitted files are written to `.dream/generated/<entry>/<generator>/` next to your project (add `.dream/` to `.gitignore`; `dreamer init` does). Diagnostics in generated code point at those files, so you can open and read exactly what was compiled. The language server reads the same files for go-to-definition and hover.

## Building source with `CodeBuilder`

When a generator emits multi-line Dream source, use `[CodeBuilder](../stdlib/codegen.md#codebuilder)` instead of manual indent strings:

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

## Caching and speed

- **Executables** are built once per generator module and compiler, and cached under `~/.dream/cache/generators/exe/`. The cache key is location-independent, so moving a project keeps its hits.
- `**@incremental` results** are replayed when the generator executable and its input snapshot are unchanged, and are still validated like a fresh run.
- **Parallelism:** independent generator builds and runs execute in parallel; `DREAM_GEN_JOBS=N` caps the worker count.
- **Prewarm:** `dream generate --prewarm` builds every standard-library generator executable ahead of time. `dreamer toolchain install` runs it for you, so the first `@json` build doesn't pay the generator build cost.

`dream -v build app.dream` prints one line per generator: `skipped`, `exe hit` / `exe miss (build …)`, `run …`, or `result hit`.

## The `dream generate` command

```bash
dream generate app.dream                         # list generators, triggers, and whether each runs
dream generate --explain json_derive app.dream   # each cache-key component and what changed
dream generate --capture json_derive app.dream   # save snapshot + result to target/generators/json_derive
dream generate --replay target/generators/json_derive   # rerun the capture and diff against its result
dream generate --verify-incremental app.dream    # rerun cached @incremental generators and diff results
dream generate --prewarm                         # build std generator executables into the cache
```

`--capture` and `--replay` let you reproduce a generator bug without the rest of the program: commit the capture directory, then replay it after changing the generator.

## Debugging a generator

A generator is a normal Dream program, so you can set breakpoints in it, including in stdlib generators such as `json_derive`. The executable is built with debug info, and stdlib sources are written to disk so breakpoints bind.

- **VS Code:** a **▶ Debug generator** code lens appears above each `@generator` function. It launches the generator on the snapshot of the program that triggers it. The *Dream: Debug generator* launch-configuration snippet does the same from `launch.json` (`"generator": "<name>"`, optional `"snapshot": "<path>"`).
- **CLI / other DAP clients:** `dream debug-adapter --generator <name> app.dream` serves DAP over stdio for that generator. Add `--snapshot <file or --capture dir>` to debug a captured input instead of rebuilding it.
- **Inside a build:** `dream build --debug-generator <name> app.dream` serves a DAP session on the generator, then finishes the build with its result.

If a generator crashes, the build reports the generator name and writes a crash report that includes the snapshot path, so you can rerun it with `--snapshot`.

## Checklist

1. Decide **replace** (DSL) or **emit** (derive).
2. Write `@generator` plus a trigger: `@syntax_block` on `fun name(ctx: GenContext)`, `@on_attribute(A)`, or `@on_call(f)`.
3. For replace generators, call `ctx.syntax_blocks()` → `ctx.replace` / `ctx.error`. For emit generators, call `ctx.types_with<A>()` → `ctx.emit_extend` / `ctx.emit_file`.
4. Declare attributes as `@attribute(AttributeTarget.X) public struct name { … }`.
5. Register via `[[generators]]` in `dream.toml` or an `import` of the generator module.
6. Mark pure generators `@incremental`.
7. Add a sample under `sample/generators/` or a golden test under `tests/cases/`.

## See also

- `[system.codegen](../stdlib/codegen.md)` — `CodeBuilder`, `GenContext`, declaration model
- [JSON](../stdlib/json.md) — `@json` derive
- `[sample/generators/quote/](https://github.com/sps014/dream/tree/main/sample/generators/quote)`
- `[sample/generators/html/](https://github.com/sps014/dream/tree/main/sample/generators/html)`
- `[sample/generators/dto/](https://github.com/sps014/dream/tree/main/sample/generators/dto)`

