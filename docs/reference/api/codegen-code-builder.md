# CodeBuilder

**Import:** `import system.codegen;`

## `public class CodeBuilder`

Accumulates Dream source lines for generator `emit_extend` / `emit_file` bodies. Indentation mirrors C# / Rust `IndentedTextWriter`: `indent()` / `dedent()` adjust depth; `line` / `append` apply the current indent only at the start of a line.

```dream
public class CodeBuilder
```

## `public constructor()`

Creates a builder that indents with four spaces.

```dream
public constructor()
```

## `with_spaces`

Creates a builder whose indent unit is `spaces` ASCII spaces.

```dream
public static fun with_spaces(spaces: int): CodeBuilder
```

## `indent`

Increases the indent depth by one unit.

```dream
public fun indent(): void
```

## `dedent`

Decreases the indent depth by one unit (no-op at zero).

```dream
public fun dedent(): void
```

## `line`

Writes `text` as a full line at the current indent, then starts a new line.

```dream
public fun line(text: string): void
```

## `append`

Appends `text` without a trailing newline (indent applied only at line start).

```dream
public fun append(text: string): void
```

## `to_string`

Returns the accumulated source text.

```dream
public override fun to_string(): string
```
