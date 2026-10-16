---
title: Formatting
summary: "Fill message patterns with values using `Fmt.format`."
icon: material/format-letter-case
api: reference/api/system-text.md
---

# Format text with placeholders

Use `Fmt.format` when the same message pattern needs different values. Use string interpolation for a short message written directly in your code.

**Import:** `import system.text;`

```dream
import system;
import system.text;

fun main() {
    let args: object[] = ["Ada", 3];
    System.println(Fmt.format("Hello {0}, you have {1} items.", args));
}
```

Output: `Hello Ada, you have 3 items.`

## Placeholder rules

`{0}` selects the first value in the argument array, `{1}` the second, and so on. Values are converted to text.

| Pattern text | Output |
| --- | --- |
| `{N}` | Argument `N` (counting from zero) as text. |
| `{N:.D}` | A `double` or `float` argument rounded to exactly `D` decimal places. |
| `{{` | A literal `{`. |
| `}}` | A literal `}`. |
| `{N}` with a non-numeric or out-of-range `N` | The placeholder text unchanged, such as `{5}`. |
| `{` with no closing `}` | A literal `{`. |

`.D` is the only supported format specification. On a non-floating-point argument, or with any other specification after the colon, the value is written with its usual text representation.

The signature is `Fmt.format(pattern: string, args: object[]): string`. The pattern and values are separate, so you can reuse a message without rebuilding its wording.

See [Fmt reference](../../reference/api/text-fmt.md) and [Strings](string.md).
