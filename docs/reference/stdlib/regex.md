# Regex

**Import:** `import system.text;`

Dream uses **one** vendored [PCRE2](https://github.com/PCRE2Project/pcre2) **16-bit** engine
(version 10.45 under `crates/dream-mir/src/runtime/c/pcre2/`).

| Backend | How it runs |
| --- | --- |
| Native C (`dream --native-c`) | PCRE2-16 with **JIT** (sljit) |
| WASM (`dream run` / `.wasm`) | Same sources, **interpreter only** (`-DPCRE2_WASM`, no W^X in linear memory) |

Invalid patterns compile to a dead engine (handle `0`) that never matches. The stdlib
`Regex` type is a thin `@intrinsic` wrapper; there is no in-language Pike/backtracker.

Do **not** hand-edit `crates/dream-mir/src/runtime/regex.wat`. Change
`crates/dream-mir/src/runtime/c/regex.c` (and/or vendored PCRE2), then:

```bash
scripts/build-runtime.sh   # wasi-sdk 33; see crates/dream-mir/src/runtime/c/pcre2/README.md
```

That compiles PCRE2 + the wrapper and writes `runtime/regex.wat`. The emitter splices that file
only when the program uses `regex_*` intrinsics.

```dream
import system;
import system.text;

fun main() {
    let digits = Regex("\\d+", RegexFlags.Global);
    System.println(digits.test("abc123"));
    System.println(digits.replace("a1b2c3", "#"));
}
```

## Flags

Combine with `|`. Default is `RegexFlags.None`.

| Flag | Meaning |
| --- | --- |
| `Global` | replace / match every occurrence |
| `IgnoreCase` | case-insensitive |
| `Multiline` | `^` / `$` at line edges |
| `DotAll` | `.` matches newlines |

## Methods

| Call | Meaning |
| --- | --- |
| `Regex(pattern, flags)` | compile |
| `test(input)` | any match? |
| `replace(input, replacement)` | substitute (`$&`, `$1`, `${name}`, `$$`) |
| `match(input)` | `string[]` of matches |
| `match_info(input)` | `Option<RegexMatchInfo>` with groups and names |

The `regex_find` microbench uses Global `[a-z]+\d+` (not a dedicated digit-run fast path).

A full example: [`sample/interop/regex.dream`](https://github.com/sps014/dream/blob/main/sample/interop/regex.dream).
