# Quote sample

This small generator turns `quote { ... }` into a Dream string while the program is built. It is a good first example for learning generator registration and replacement.

## About this example

Small build-time `quote { … }` custom notation: opaque text in the braces becomes a Dream string literal.

## User-facing code

```dream
import system;

fun main() {
    System.println(quote { Hello generators });
}
```

## Run

```bash
# from repo root
cargo run -- run sample/generators/quote/app.dream
```

Expected stdout:

```text
Hello generators
```

## Layout

| File | Role |
|------|------|
| `app.dream` | Program that uses `quote { … }` |
| `gen.dream` | `@generator` + `@syntax_block` (block keyword = function name), executed with a `GenContext` |
| `dream.toml` | `[[generators]] path = "gen.dream"` |

Start here before the larger [`../html/`](../html/) sample. See [Source generators](../../../docs/reference/language/generators.md).
