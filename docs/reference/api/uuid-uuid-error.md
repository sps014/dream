# UuidError

**Import:** `import system.uuid;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `UuidError`

Invalid UUID text or an unavailable secure entropy source.

```dream
public class UuidError: Error
```

## `constructor`

Creates a UUID error without including secret input.

```dream
public constructor(message: string)
```

## `code`

Stable UUID error code.

```dream
public fun code(): string
```

## `message`

Human-readable validation or entropy failure.

```dream
public fun message(): string
```
