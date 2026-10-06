# CodeBuilder (`system.codegen`)


CodeBuilder helps you write formatted Dream source. GenContext helps a registered source generator inspect declarations, replace custom notation, and report errors. Start with the [generator guide](../language/generators.md) before using the reference below.

**Import:** `import system.codegen;`

Helpers for [source generators](../language/generators.md) you write in Dream. Cookbook: [Quote syntax](../../cookbook/quote-generator.md).

## `CodeBuilder`

Builds indented Dream source. Construct with `CodeBuilder()` (4 spaces) or `CodeBuilder.with_spaces(n)`.

```dream
import system.codegen;

fun snippet(): string {
    let b = CodeBuilder();
    b.line("public fun to_json(): JsonValue {");
    b.indent();
    b.line("return JsonValue.dict();");
    b.dedent();
    b.line("}");
    return b.to_string();
}
```

`indent()` / `dedent()`, `line(text)`, `append(text)`, `to_string()`.

## GenContext

`GenContext` is the compile-time handle a `@generator` function receives. Use it to find the sites that triggered the generator, queue output, and report errors. Full walkthrough: [Source generators](../language/generators.md).

| Call | Meaning |
| --- | --- |
| `syntax_blocks()` | `name { … }` sites for an `@syntax_block` generator |
| `types_with<A>()` / `functions_with<A>()` | declarations carrying `@A` |
| `call_sites()` | calls of the generator's `@on_call` functions |
| `type_index()` / `find_type(name)` | every type in the program, by name and `DeclKind` |
| `options<T>()` / `option_or(key, fallback)` | `dream.toml` `[[generators]].options` |
| `replace(block, source)` | swap a syntax site before type-check |
| `emit_extend(decl, body)` / `emit_file(path, source)` | add members or a whole file |
| `error(node, message)` / `warning(decl, message)` / `log(message)` | diagnostics and `-v` log lines |

## Declaration model

`GenDecl` (`kind`, `fields`, `variants`, `methods`, `generics`, `attributes`) describes one declaration. `GenField.ty` is a `GenTypeRef`: query it with `primitive()`, `collection()`, `is_option()`, `arg(i)`, and `is_decl(declof(T))` instead of comparing type names as strings. Read typed attributes with `has<A>()`, `attribute<A>()`, and `attributes_of<A>()`.

Samples: [`quote`](https://github.com/sps014/dream/tree/main/sample/generators/quote), [`html`](https://github.com/sps014/dream/tree/main/sample/generators/html), [`dto`](https://github.com/sps014/dream/tree/main/sample/generators/dto). See [all generator declarations](../api/system-codegen.md).
