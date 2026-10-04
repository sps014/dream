# CodeBuilder, GenHost

**Import:** `import system.codegen;`

Read the [usage guide](../stdlib/codegen.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class CodeBuilder`

Accumulates Dream source lines for generator `emit_extend` / `emit_file` bodies.

```dream
public class CodeBuilder
```

## `constructor`

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

## `static class GenHost`

Stdout protocol markers and helpers for Dream-side generator harnesses.

```dream
public static class GenHost
```

## `ok_marker`

```dream
public static fun ok_marker(): string
```

## `err_marker`

```dream
public static fun err_marker(): string
```

## `loc_marker`

Optional location line after an error marker: `type_name\tfield_name`.

```dream
public static fun loc_marker(): string
```

## `emit_extend_marker`

Marker before an `emit_extend` payload in a successful harness run.

```dream
public static fun emit_extend_marker(): string
```

## `emit_file_marker`

Marker before an `emit_file` payload in a successful harness run.

```dream
public static fun emit_file_marker(): string
```

## `format_loc`

Formats a span hint the host can attach to a diagnostic (empty field allowed).

```dream
public static fun format_loc(type_name: string, field_name: string): string
```
