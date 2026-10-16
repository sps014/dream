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


Triggers are paths, not strings. `@on_attribute(jsn)` is a compile error (`'jsn' is not a declared @attribute type`), and so is naming something that is not an `@attribute` struct. A generator may combine several triggers; `@json` uses `@on_attribute(JsonAttribute)` and `@on_call(Json.serialize, Json.deserialize, Json.from_value)`.

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
Extensions retain the target declaration's module and internal visibility. Generated files
may import already-loaded modules and bind their functions using explicit import aliases;
they cannot load new source dependencies.

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
- `GenTypeRef.display` is a diagnostic spelling and can contain canonical module qualification. Use `source_name()` in a scope importing the declaration, or emit explicit type import aliases when generating a different module.
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


## Attribute contracts and enum arguments

Attributes are declared in Dream modules. Their fields define positional arguments, and
`@attribute(AttributeTarget.X)` declares their allowed placements. Compiler effects such as
inlining and C calling conventions are implemented by the compiler using the declaration's
module identity.

Core attributes need no import. Import `system.interop` for C/C++ attributes,
`system.testing` for `@test` and `@timeout`, and `system.codegen` for generator attributes.
JSON attributes remain in `system.json`.

Finite choices use enums rather than strings:

```dream
import system.interop;

@c("user32", "MessageBoxW")
@c_call(CCallConv.Stdcall)
@marshal(MarshalKind.LpWStr)
extern fun message_box(window: CPtr, text: string, title: string, flags: int): int;
```

`MarshalKind.UserDataLast` applies to `NativeCallback` parameters. `LpStr` and `LpWStr`
apply to C extern declarations. Compiler intrinsic declarations use
`@intrinsic(Intrinsic.Print)` and the other members of `Intrinsic`. Library paths and
external symbol names remain strings.

A trailing `Option<T>` field makes an argument optional: omission decodes as `None`, and
a supplied value as `Some(value)`. Required fields cannot follow optional fields. A trailing
`List<T>` field collects remaining arguments; it cannot be combined with optional fields.
A trailing array is also variadic, allowing the bootstrap `attribute` declaration to use
`AttributeTarget[]` without depending on `system.collections`.

Generator triggers use `DeclarationPath`, which stores the dotted declaration reference in
its `text` field. Its `symbol` field carries the canonical compiler identity resolved in
that source file, including imported function aliases; unresolved references have an empty
identity. Attribute-only references participate in `referenced_functions()` and
`referenced_decls()`, so `ctx.find_function(path.symbol)` can inspect dependency signatures.
For example, `@on_call(Json.serialize)` supplies a declaration path rather than an enum member.
Unknown trigger paths are diagnosed by the generator framework.

An attribute type may use an `Attribute` suffix; its applied name removes the suffix and
converts the remaining name to snake_case. For example, `JsAttribute` is applied as `@js` and remains
available as `JsAttribute` for typed decoding. This avoids a collision with the primitive
`js` type.

## Inspecting the input compilation

A generator receives syntax bodies for its selected user functions and their transitive
referenced functions. Embedded standard-library functions expose signatures only.
`GenFunction.body` is an `Option<GenBody>`; statements and expression children carry stable
identities and source locations. `GenExpr.ty`, `symbol`, `overload`, `type_args`, and
`constant` describe facts inferred before generation. Unresolved types are
`TypeRefKind.Unknown`; an unresolved symbol is the empty string. A missing constant means
that no exact scalar value was inferred.

The compiler performs this analysis with scratch diagnostics and without HIR emission.
It runs only when a generator applies. `ctx.compilation_errors()` exposes errors restricted
to the generator's selected declarations. Normal analysis runs again after generated output
has been merged, so references to generated code can succeed then.

Use `ctx.referenced_decls()`, `ctx.referenced_functions()`, `ctx.find_decl(id)`, and
`ctx.find_function(id)` to inspect dependencies. The closure is deterministic and limited
to 512 declarations and 32 levels. `ctx.find_symbol(id)` searches program-wide signature
views; `implementations_of(id)` and `subtypes_of(id)` traverse declared interface ancestry.

`ctx.consume(function)` removes a selected or referenced top-level user function from the
compiled program. Standard-library functions and methods cannot be consumed. A later use
of a consumed function produces a diagnostic naming the generator that consumed it.
Two generators cannot consume the same function.

## Artifacts and source maps

`ArtifactBuilder` collects text and source mappings independently of Dream source emission:

```dream
let artifact = ArtifactBuilder();
artifact.mapped("external text", function.id);
ctx.emit_artifact("input.txt", artifact.to_string(), artifact.source_map);
```

Artifact paths are relative to the generator directory, cannot contain `..`, and cannot
end in `.dream`. `outputs.json` is reserved for the compiler. Artifacts are materialized
under `.dream/generated/<entry>/<generator>/` without a generated-source header.
Pass `publish: true` to `emit_artifact` to also copy an artifact beside the build output.
Published artifacts cannot overwrite source inputs or compiler outputs.

Source-map ranges use one-based lines and Unicode-character columns and exclude their end.
`ctx.error_in_artifact(path, line, column, message)` selects the innermost covering mapping
and reports the error at its originating Dream node. An uncovered point stays anchored to
the artifact file. `ctx.error_at(node.id, message)` also accepts body-node identities.

## Pinned tools and feedback phases

A generator declares each external tool with `@uses_tool("name")`. Tools resolve from the
pinned LLVM/Binaryen toolchain or from a project declaration:

```toml
[tools.schema]
path = "tools/schema-checker"
version = "1.0"
hash = "<64 hexadecimal characters: BLAKE3 of the executable>"
```

Tool declarations are fingerprinted before cache lookup. An undeclared tool or an
executable whose hash differs from its project pin is rejected.

`ctx.run_tool(id, tool, args, inputs, outputs)` takes lists of arguments and artifact paths.
The compiler copies declared inputs into a scratch directory, invokes the tool directly,
and collects declared outputs. macOS uses `sandbox-exec`; Linux requires Bubblewrap
(`bwrap`). Tool execution fails on hosts without a supported sandbox. Network access is
disabled, the environment is minimal, and a tool has a 30-second timeout. Captured stdout
and stderr are limited to 8 MiB each; individual output files are limited to 64 MiB.

A generator starts at `ctx.phase() == 0`. After tool requests, the compiler invokes the
same executable with the next phase number and `ctx.tool_results()`. Results include the
request identity, exit code, stdout, stderr, and output files. Binary files use base64
contents with `binary == true`; every file includes a BLAKE3 hash. The generator interprets
the tool's diagnostics and emits the final Dream files or assets. There are at most three
phases; the final phase cannot request another tool. Request identities and output paths
must be unique across phases.

Incremental results include the phase transcript and tool feedback. Cache replay and the
materialized manifest validate the same paths, outputs, mappings, and consumption targets
as fresh execution. Capture/replay and incremental verification retain phase data.

## Embedded assets

`ctx.emit_asset(name, source)` embeds an artifact or declared tool-output file.
The overload taking `targets: List<string>` restricts an asset to target triples or the
families `native` and `wasm`. Asset names must be unique across generators.

At runtime, `Assets.bytes("name")` copies the named resource into an owned `byte[]`.
The name must be a string literal; unknown names are compiler diagnostics. Constant byte
payloads use private immutable LLVM globals, which become data segments on WebAssembly.
Selective JS runtimes also export `embeddedAssets`, a map of names to `Uint8Array` values.
Ordering follows generator name and output emission order.

The compiler persists each generator's `outputs.json` manifest with the materialized
sources. Replay-only builds and the language server read the manifest, including
replacements, consumption, artifacts, source maps, and assets.
