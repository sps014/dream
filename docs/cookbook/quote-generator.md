# Source generator: `quote { … }`

A generator runs **at compile time**. This one turns `quote { Hello }` into the string `"Hello"`.

You need three files in one project.

**`dream.toml`** — register the generator:

```toml
[[generators]]
path = "gen.dream"
```

**`gen.dream`** — the generator:

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

**`src/main.dream`** (or `app.dream`):

```dream
import system;

fun main() {
    System.println(quote { Hello generators });
}
```

```
Hello generators
```

```bash
dreamer run
# in this repo:
cargo run -- run sample/generators/quote/app.dream
```

- `@syntax_block("quote")` — the compiler hands you every `quote { … }` site.
- `ctx.replace` — swap that site for ordinary Dream source **before** type-checking.

Bigger samples: [`html`](https://github.com/sps014/dream/tree/main/sample/generators/html), [`dto`](https://github.com/sps014/dream/tree/main/sample/generators/dto). Reference: [Source generators](../reference/language/generators.md), [CodeBuilder](../reference/stdlib/codegen.md).
