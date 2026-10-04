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

`{0}` selects the first value in the argument array, `{1}` the second, and so on. Values are converted to text. `{{` and `}}` write literal braces.

For a floating-point argument, `{0:.2}` requests two decimal places. An invalid or out-of-range argument index keeps its placeholder text. An unsupported format specification falls back to the value's usual text representation.

The signature is `Fmt.format(pattern: string, args: object[]): string`. The pattern and values are separate, so you can reuse a message without rebuilding its wording.

See [Fmt reference](../api/text-fmt.md) and [Strings](string.md).
