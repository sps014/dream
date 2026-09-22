# CodeBuilder (`system.codegen`)

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

`GenContext` is the compile-time handle a `@generator` function receives — find syntax sites, replace them, and report errors. Full walkthrough: [Source generators](../language/generators.md).

## GenSyntaxBlock

Each `quote { … }` (or other `@syntax_block`) site is a `GenSyntaxBlock` (`.name`, `.body`, splices).

| Call | Meaning |
| --- | --- |
| `syntax_blocks(name)` / `all_blocks()` | matching sites |
| `replace(block, dream_source)` | swap the site before type-check |
| `error(block, message)` | fail compilation |

## `GenFieldInfo`

`GenFieldInfo` is one field on a snapshotted declaration type (`.name`, `.type_name`) — used by emit-style generators.

`@json` is a compiler builtin, not a `GenContext` generator — see [JSON](json.md).

Samples: [`quote`](https://github.com/sps014/dream/tree/main/sample/generators/quote), [`html`](https://github.com/sps014/dream/tree/main/sample/generators/html), [`dto`](https://github.com/sps014/dream/tree/main/sample/generators/dto).
