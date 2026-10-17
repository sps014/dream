# CliError

**Import:** `import system.cli;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `CliError`

Invalid invocation, command definition, or typed lookup; help and version are successful outcomes.

```dream
public class CliError: Error
```

## `constructor`

Creates an error with a stable machine code and diagnostic text.

```dream
public constructor(code: string, message: string)
```

## `code`

EINVAL for definitions, EARGS for invocations, ENOENT for missing values, ETYPE for incorrect getters, or EIO for adapter failures.

```dream
public fun code(): string
```

## `message`

Human-readable diagnostic; invocation failures include usage guidance.

```dream
public fun message(): string
```
