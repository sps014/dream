# HTML sample

Compile-time `html { <tags>… }` syntax DSL. Prefer **quote** if you are learning generators.

## User-facing code

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

## Run

```bash
# from repo root
cargo run -- run sample/generators/html/app.dream
```

Expected stdout:

```text
<div class="hero"><h1>Hello</h1><p>Welcome</p></div>
```

## Layout

| File | Role |
|------|------|
| `app.dream` | Program that uses `html { … }` |
| `html.dream` | Runtime `Html.el` / `render` / `text` |
| `parser.dream` | `HtmlCompiler` — markup → Dream `Html.el` source |
| `gen.dream` | `@generator(ctx: GenContext)` + `@syntax_block("html")` |
| `dream.toml` | `[[generators]] path = "gen.dream"` |

See [Source generators](../../../docs/language/generators.md).
