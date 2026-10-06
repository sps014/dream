# HTML sample

This example turns `html { ... }` notation into text while the program is built. Try the smaller quote generator first, then use this example to explore structured custom notation.

## About this example

Compile-time `html { <tags>… }` custom notation. Prefer **quote** if you are learning generators.

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
| `gen.dream` | `@generator(ctx: GenContext)` + `@syntax_block` (block keyword = function name) |
| `dream.toml` | `[[generators]] path = "gen.dream"` |

See [Source generators](../../../docs/reference/language/generators.md).
