---
title: Source generators
summary: "Write code that writes code, or add a small custom notation."
icon: material/cog-transfer-outline
status: stable
targets: [native, web, node]
api: reference/api/system-codegen.md
related: [library/codegen/codebuilder.md, cookbook/quote-generator.md]
---

# Source generators

A source generator writes or replaces Dream code while the program builds, for repeated code or a small custom notation. Dream does not inspect arbitrary type definitions at runtime.

!!! glance "At a glance"

    ```dream
    System.println(
        quote { Hello generators }
    );
    ```

    - A generator writes or replaces Dream code while your program builds.
    - Generators ship as ordinary packages, like `@json` in `system.json`.

A generator is an ordinary Dream function, compiled once and cached. It runs before type-checking, in the same environment as the build, and it is not sandboxed.

A generator from a dependency can read files and start programs, the same trust you give a build script. `dreamer install` warns when a dependency ships one.

A generator does one of two things:

1. **Replace** custom syntax — `quote { … }` or `html { … }` become ordinary Dream expressions at call sites. Use this when the braced body is a domain-specific shape the main parser does not understand.
2. **Emit** new Dream source — for example `@json` adds `to_json` / `from_json` to your types. Use this when call sites stay ordinary Dream and the generated code is methods or helpers.

API details live in [system.codegen](../../library/codegen/codebuilder.md). The shipped `@json` derive is itself a Dream generator in `system.json`; see [JSON](../../library/serialization/json.md).

## Your first generator: `quote`

`quote { … }` turns the text inside the braces into a Dream string literal at compile time.

```dream
import system;

fun main() {
    System.println(
        quote { Hello generators }
    );
}
```

```bash
dream run app.dream
```

Expected stdout: `Hello generators`

Full sample: [sample/generators/quote/](https://github.com/sps014/dream/tree/main/sample/generators/quote).

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

The function takes one `GenContext`. Queue rewrites with `ctx.replace` and failures with `ctx.error`, and they are checked before type-checking.

## Triggers

A generator runs only when the program contains something it asked for. If nothing matches, the generator is not built or launched, so importing one costs nothing until you use it.


| Attribute          | Runs when                                              | Context API                                                                 |
| ------------------ | ------------------------------------------------------ | --------------------------------------------------------------------------- |
| `@syntax_block`    | a `name { … }` site uses the generator's function name | `ctx.syntax_blocks()`                                                       |
| `@on_attribute(A)` | a declaration (or one of its members) or a `module` declaration carries `@A` | `ctx.types_with<A>()`, `ctx.functions_with<A>()`, `ctx.modules_with<A>()`, `ctx.decls()` |
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
dream run app.dream
```

HTML is **not** a language builtin. The sample's `gen.dream` calls `HtmlCompiler` (in `parser.dream`) from its generator body, so the complexity stays in helper modules.

Full sample: [sample/generators/html/](https://github.com/sps014/dream/tree/main/sample/generators/html).


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
public fun list_users(): void {}
```

Inside a generator, read attributes as typed values: `decl.has<route>()`, `decl.attribute<route>()` (an `Option<route>` with `path` filled in), and `attributes_of<A>()` for repeated attributes.

## Emit-style derives

Emit generators read declarations and queue new source with `ctx.emit_extend(decl, body)` or `ctx.emit_file(path, source)`. An extension keeps the target's module and internal visibility.

A generated file may import modules that are already loaded and bind their functions with explicit aliases. It cannot load a new source dependency.

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

Full sample: [sample/generators/dto/](https://github.com/sps014/dream/tree/main/sample/generators/dto).

The declaration model is typed:

- `GenDecl` carries `kind` (`DeclKind.Class` / `Struct` / `Union` / …), `fields`, `variants`, `methods`, `constructors`, and `generics`.
- `GenTypeRef.display` is a diagnostic spelling and can contain canonical module qualification. Use `source_name()` in a scope importing the declaration, or emit explicit type import aliases when generating a different module.
- `GenField.ty` is a `GenTypeRef`. Use `primitive()` (`Primitive.Int`, …), `collection()` (`CollectionKind.List`, …), `is_option()`, and `arg(i)` instead of comparing type names as strings.
- `ctx.type_index()` lists every type in the program by name and kind, and `ctx.find_type(name)` looks one up.
- `ctx.options<T>()` decodes `dream.toml` `[[generators]].options` into an `@json` type, and `ctx.additional_files()` returns the files listed in `additional_files`.

### `declof`

`declof(path)` evaluates to the identity string of a type, function, field, method, or variant. Use it to compare against model identities, such as `GenCallSite.callee` or `GenTypeRef.is_decl(id)`, without spelling them by hand:

```dream
let serialize = declof(Json.serialize); // "system.json::Json.serialize"
for (let call in ctx.call_sites()) {
    if call.callee == serialize { /* ... */
    }
}
```

A path that resolves to nothing is a compile error.

1. Generated files

Emitted files go to `generated/<entry>/<module-path>/<generator>/` next to the project. Add `generated/` to `.gitignore`; `dreamer init` does this for you.

Diagnostics in generated code point at those files, and the language server uses them for go-to-definition and hover. The path is one directory per module segment, omitted for an unnamed module, and a file with no `dream.toml` gets `generated/` beside the entry file.

Generator executables live in `target/obj/generators/exe/<key>/` under the same directory. Deleting it rebuilds them next time, and unchanged incremental results stay cached.

Generators in different modules may share a name. A syntax block finds its generator through the file's module, imports, and visibility, and an ambiguous name is an error.

`dream generate` accepts a `module::name` identity, such as `dream generate main.dream --explain system.json.derive::json_derive`. A short name works only when it matches one generator.

Emitted files use the module declared in their source, and generated extension methods can access the extended type's private members. Output may also use declarations selected for generation and their dependencies, such as a web route's dependency provider.

Unrelated file-private functions and internal declarations stay inaccessible.

## Building source with `CodeBuilder`

When a generator emits multi-line Dream source, use [CodeBuilder](../../library/codegen/codebuilder.md#codebuilder) instead of manual indent strings:

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
- **Incremental results:** `@incremental` results are replayed when the generator executable and its input snapshot are unchanged, and are still validated like a fresh run.
- **Parallelism:** independent generator builds and runs execute in parallel; `DREAM_GEN_JOBS=N` caps the worker count.
- **Prewarm:** `dream generate --prewarm` builds every standard-library generator executable ahead of time. `dreamer toolchain install` runs it for you, so the first `@json` build doesn't pay the generator build cost.

`dream -v build app.dream` prints one line per generator: `skipped`, `exe hit` / `exe miss (build …)`, `run …`, or `result hit`.

## The `dream generate` command

```bash
dream generate app.dream                         # list generators, triggers, and whether each runs
dream generate --explain json_derive app.dream   # each cache-key component and what changed
dream generate --capture json_derive app.dream   # save snapshot + result to target/obj/generators/captures/json_derive
dream generate --replay target/obj/generators/captures/json_derive   # rerun the capture and diff against its result
dream generate --verify-incremental app.dream    # rerun cached @incremental generators and diff results
dream generate --prewarm                         # build std generator executables into the cache
```

`--capture` and `--replay` let you reproduce a generator bug without the rest of the program: commit the capture directory, then replay it after changing the generator.

## Debugging a generator

A generator is a normal Dream program, so you can set breakpoints in it, including in `json_derive`.

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
7. Add tests in your project for successful generation, invalid input, and diagnostics.

## See also

- [system.codegen](../../library/codegen/codebuilder.md) — `CodeBuilder`, `GenContext`, declaration model
- [JSON](../../library/serialization/json.md) — `@json` derive
- [sample/generators/quote/](https://github.com/sps014/dream/tree/main/sample/generators/quote)
- [sample/generators/html/](https://github.com/sps014/dream/tree/main/sample/generators/html)
- [sample/generators/dto/](https://github.com/sps014/dream/tree/main/sample/generators/dto)


## Attribute contracts and enum arguments

Attributes are declared in Dream modules. Their fields are the positional arguments, and `@attribute(AttributeTarget.X)` says where the attribute may appear.

Built-in effects such as inlining and C string encoding use that declaration. Core attributes need no import. `@owned` lives in `system.interop` with the other C attributes.

Import `system.interop` for C and C++ attributes, `system.testing` for `@test` and `@timeout`, and `system.codegen` for generator attributes. JSON attributes stay in `system.json`.

Finite choices use enums rather than strings:

```dream
import system.interop;

@c("user32", "MessageBoxW")
@marshal(StringEncoding.Utf16)
extern fun message_box(window: CPtr, text: string, title: string, flags: int): int;
```

`StringEncoding.Utf8` and `StringEncoding.Utf16` apply to `@c` externs. `UserDataPosition.First`
and `UserDataPosition.Last` apply to `NativeCallback` parameters through `@user_data`. Compiler intrinsic declarations use
`@intrinsic(Intrinsic.Print)` and the other members of `Intrinsic`. Library paths and
external symbol names remain strings.

A trailing `Option<T>` field is optional: omission is `None`, and a value is `Some(value)`. A required field cannot follow an optional one.

A trailing `List<T>` collects the remaining arguments and cannot be combined with optional fields. A trailing array is variadic too, so the bootstrap `attribute` declaration can use `AttributeTarget[]` without `system.collections`.

Generator triggers use `DeclarationPath`: `text` is the dotted name you wrote, and `symbol` is the resolved identity, including an imported alias. An unresolved reference has an empty `symbol`.

Attribute-only references appear in `referenced_functions()` and `referenced_decls()`, so `ctx.find_function(path.symbol)` can read a dependency's signature. `@on_call(Json.serialize)` is a declaration path, not an enum member, and an unknown trigger path is an error.

An attribute type may end in `Attribute`, and the applied name drops that suffix into snake_case. `JsAttribute` is written `@js` and remains `JsAttribute` when you decode it, so it does not collide with the `js` type.

## Inspecting the input compilation

A generator sees the syntax of the user functions it selected and of the functions those call. Embedded standard-library functions expose signatures only.

`GenFunction.body` is an `Option<GenBody>`, and each statement and expression child has a stable identity and a source location. `GenExpr.ty`, `symbol`, `overload`, `type_args`, and `constant` describe what was already inferred.

An unresolved type is `TypeRefKind.Unknown`, and an unresolved symbol is the empty string. A missing constant means no exact scalar was inferred.

`ctx.compilation_errors()` reports errors in the generator's selected declarations.
References to code that the generator has yet to produce may be unresolved in this view;
they are checked again after the generated output is added.

Use `ctx.referenced_decls()`, `ctx.referenced_functions()`, `ctx.find_decl(id)`, and `ctx.find_function(id)` to inspect dependencies. That set is limited to 512 declarations and 32 levels.

`ctx.find_symbol(id)` searches the whole program's signatures. `implementations_of(id)` and `subtypes_of(id)` follow declared interface parents.

`ctx.consume(function, help)` takes over a selected or referenced top-level user function, for example one the generator translates. The function stays in the program and is type-checked like any other code, so you do not forward `ctx.compilation_errors()`.

The function is never compiled. Calling it, or using it as a value, from ordinary code is an error that names the generator. The optional `help` text appears under that error:

```dream
ctx.consume(f, "dispatch it with Shaders." + f.name + ".run(...)");
```

```text
error: 'blur' was consumed by generator 'shaders' and is not available as a function
help: dispatch it with Shaders.blur.run(...)
```

A consumed function may still call other consumed functions, so one generator can translate a group. Extra rules, such as "no closures", stay the generator's job.

Standard-library functions and methods cannot be consumed, and two generators cannot consume the same function. Syntax blocks inside a consumed body belong to the generator that consumed it.

## Configuration on modules

Generator settings belong in source, next to the code they configure. An `@attribute` type
whose targets include `AttributeTarget.Module` can be placed on a `module` declaration, and
`@on_attribute` fires for it just like for a function or type:

```dream
@kernel(64)
module shaders;

import gpu;

@kernel(128)
public fun blur(x: float): float {
    return x * 0.5;
}

@kernel(0)
public fun fill(x: float): float {
    return x;
}
```

`ctx.modules_with<A>()` lists the modules that carry `@A`, and `ctx.module_of(decl)` or
`ctx.module_of(function)` returns the trigger-carrying module that declares a declaration, so
a generator can merge module defaults with per-declaration overrides:

```dream
let size = 0;
switch (ctx.module_of(f)) {
    Some(m) => {
        switch (m.attribute<KernelAttribute>()) {
            Some(defaults) => {
                size = defaults.workgroup;
            }
            None => {}
        }
    }
    None => {}
}
switch (f.attribute<KernelAttribute>()) {
    Some(own) => {
        if own.workgroup > 0 {
            size = own.workgroup;
        }
    }
    None => {}
}
```

`GenModule` exposes `attribute<A>()`, `attributes_of<A>()` and `has<A>()`. Its `id` is a
diagnostic target, so `ctx.error_at(m.id, message)` points at the module declaration.

## Generator-only declarations

Some names exist only in a generator's target language, such as a shader's thread index.
Declare them with `@generator_only(A, ...)` from `system.codegen`. The compiler type-checks
uses like any other declaration, so generators see real types, symbols and generic arguments
in the body facts, but the declarations are never compiled:

```dream
module gpu;

import system.codegen;

@attribute(AttributeTarget.Module, AttributeTarget.Function)
public struct KernelAttribute {
    public workgroup: int;
}

@generator_only(KernelAttribute)
public extern fun global_id(): uint;

@generator_only(KernelAttribute)
public extern fun backward<F>(f: F, ref gradient: float): float;

@generator_only(KernelAttribute)
public class Texture<T> {
    public extern fun sample(u: float, v: float): T;
}
```

A generator-only declaration is a bodiless `extern` function, which may be `public` and generic, or a class, struct, or interface, whose members are covered too. Use it only inside a consumed function that carries one of the listed attributes.

Any other use is an error:

```text
error: 'global_id' is only available inside functions marked @kernel
```

Passing a consumed or generator-only function as a value keeps its normal type. In `backward(blur, ref gradient)`, `ctx.type_of` of `blur` is `fun(float): float` and `ctx.symbol_of` is that function's identity.

`ref` parameters keep `GenParam.is_ref`. A `ref` argument is an `ExprKind.Ref` node with the argument's type, so a generator can see a write through a parameter.

## Artifacts and source maps

`ArtifactBuilder` collects text and source mappings independently of Dream source emission:

```dream
let artifact = ArtifactBuilder();
artifact.mapped("external text", function.id);
ctx.emit_artifact("input.txt", artifact.to_string(), artifact.source_map);
```

Binary files, such as packed GPU code, use the `byte[]` overload and have no source map. Embed them with `emit_asset`, or pass them to `run_tool`, the same way as text:

```dream
ctx.emit_artifact("kernels.bundle", bundle_bytes);
ctx.emit_asset("kernels", "kernels.bundle");
```

Artifact paths are relative to the generator directory. They cannot contain `..` or end in `.dream`, and `outputs.json` is reserved.

Artifacts are written under `generated/<entry>/<module-path>/<generator>/` without a generated-source header. Pass `publish: true` to `emit_artifact` to copy one beside the build output, without overwriting a source file or a compiler output.

Source-map ranges use one-based lines and Unicode-character columns, and they exclude their end. `ctx.error_in_artifact(path, line, column, message)` reports the error at the Dream node that covers that point.

An uncovered point stays on the artifact file. `ctx.error_at(node.id, message)` also accepts a body-node identity.

## Pinned tools and feedback phases

A generator declares each external tool with `@uses_tool("name")`, where `name` is an
executable. Dream resolves it from its own table of pinned tools first:

| Tool       | Install with                        | Executables                                  | Override         |
| ---------- | ----------------------------------- | -------------------------------------------- | ---------------- |
| `llvm`     | ships with Dream releases           | `clang`, `opt`, `llc`, `llvm-*`, `wasm-ld`, … | `DREAM_LLVM`     |
| `binaryen` | `dreamer toolchain install binaryen` | `wasm-opt`, `wasm-as`, `wasm-dis`, …         | `DREAM_WASM_OPT` |
| `slang`    | `dreamer toolchain install slang`    | `slangc`                                     | `DREAM_SLANG`    |

Each tool installs into `~/.dream/toolchains/<name>-<version>`, and `DREAM_<TOOL>` points at that install or its `bin/` (`DREAM_WASM_OPT` names the executable). Pin any other tool in `dream.toml`:

```toml
[tools.schema]
path = "tools/schema/bin/schema-checker"
version = "1.0"
hash = "<64 hexadecimal characters: BLAKE3 of the executable>"
root = "tools/schema"
timeout_secs = 60
```

`hash` is checked before every use, and a mismatch reports the file's real hash. `root` is the readable install tree, `timeout_secs` overrides the 30-second default, and pinned-table tools keep their own limits.

`ctx.tools()` lists every declared tool. A missing tool has `available == false`, and `run_tool` then fails with a reason such as the install command:

```dream
let recorder = false;
for (let tool in ctx.tools()) {
    if tool.name == "dream-slang-layout" && tool.available {
        recorder = true;
    }
}
```

Tool declarations are fingerprinted before cache lookup. A tool with an install tree is fingerprinted over all of its files, so replacing a sibling library invalidates the cache.

An undeclared tool, or an executable whose hash differs from its pin, is rejected.

`ctx.run_tool(id, tool, args, inputs, outputs, opaque_outputs)` takes argument and artifact lists. Declared inputs are copied into a scratch directory, the tool runs there, and declared outputs are collected.

Network access is off, the environment is minimal, and the tool can write only in its scratch directory. It can read system locations, its executable, and its install tree:

- macOS runs the tool under `sandbox-exec`.
- Linux requires Bubblewrap; when `bwrap` is missing, the error explains how to install it.
- Windows runs the tool in a job object that ends every process it starts, under an
  AppContainer that can write only to the scratch directory.

Stdout, stderr, and each output file are capped (8 MiB and 64 MiB). One phase runs at most `DREAM_GEN_JOBS` requests at a time, in request order, and reports the first failure in that order.

`opaque_outputs` are collected, but the next phase sees only the path, hash, and size (`GenToolFile.opaque` is true and `contents` is empty). Use them for large or binary results the generator does not read.

Refer to a tool output as `tool:<request id>/<path>`:

```dream
ctx.emit_asset("kernels", "tool:compile/kernels.spv");
ctx.publish_tool_output("tool:compile/kernels.spv", "shaders/kernels.spv");
```

`publish_tool_output` copies the file next to the build output; the same rules as for
published artifacts apply.

A generator starts at `ctx.phase() == 0`. After tool requests, the same executable runs again with the next phase number and `ctx.tool_results()`.

A result includes the request identity, exit code, stdout, stderr, and output files. Binary files use base64 with `binary == true`, and every file includes its BLAKE3 hash and size.

Read the tool's diagnostics and emit the final Dream files or assets. There are at most three phases, the last phase cannot request another tool, and request identities and output paths must be unique.

Incremental results store a phase transcript: the first snapshot's hash, each phase's outputs and tool feedback, and every tool output's bytes, stored once by hash. Later snapshots are rebuilt from that record, so the transcript grows with the tool outputs.

Cache replay and the written manifest check the same paths, outputs, mappings, hashes, and consumed functions as a fresh run. Capture, replay, and incremental verification keep the phase data.

## Embedded assets

`ctx.emit_asset(name, source)` embeds an emitted artifact, or a tool output named `tool:<request id>/<path>`. The `targets: List<string>` overload limits an asset to target triples or to the families `native` and `wasm`.

Asset names must be unique across generators.

At runtime, `Assets.bytes("name")` copies the named resource into an owned `byte[]`. The name must be a string literal, and an unknown name is a compiler error.

Selective JS runtimes also export `embeddedAssets`, a map of names to `Uint8Array` values. Order follows generator name, then the order assets were emitted.

The compiler persists each generator's `outputs.json` manifest with the materialized
sources. Replay-only builds and the language server read the manifest, including
replacements, consumption, artifacts, source maps, and assets.

## Editor support

The language server replays the last build's manifests, so hover, go to definition, references, and completion work inside consumed functions, including generator-only names. Before the first build, a function with a granting attribute is treated as consumed.

An artifact error from `ctx.error_in_artifact` appears at the Dream code its source map points to. A consumed function shows a "consumed by generator" lens, then a lens for each artifact mapped back into it, such as a translated shader.
