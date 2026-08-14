# DTO sample

Compile-time emit generator: types marked `@dto` get a synthesized `describe()` method via
`GenContext.emit_extend`.

## User-facing code

```dream
import system;
import gen;

@dto
class Point {
    public x: int;
    public y: int;

    public constructor(x: int, y: int) {
        this.x = x;
        this.y = y;
    }
}

fun main(): void {
    let p = Point(1, 2);
    System.println(p.describe());
}
```

## Run

```bash
# from repo root
cargo run -- run sample/generators/dto/main.dream
```

Expected stdout:

```text
dto
```

## Layout

| File | Role |
|------|------|
| `main.dream` | Program using `@dto` on a class |
| `gen.dream` | `@attribute` schema + `@generator` emit logic |
| `dream.toml` | `[[generators]] path = "gen.dream"` |

See [Source generators](../../../docs/reference/language/generators.md).
