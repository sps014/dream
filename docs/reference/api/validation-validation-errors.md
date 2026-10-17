# ValidationErrors

**Import:** `import system.validation;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `ValidationErrors`

Deterministically ordered input errors without echoing sensitive submitted values.

```dream
public class ValidationErrors
```

## `issues`

Validation failures, in discovery order.

```dream
public issues: List<ValidationIssue>
```

## `constructor`

Creates a `ValidationErrors` with default settings.

```dream
public constructor()
```

## `add`

Appends an issue with the supplied location, code, and message.

```dream
public fun add(location: List<string>, code: string, message: string): void
```

## `is_empty`

Returns whether there are no elements.

```dream
public fun is_empty(): bool
```

## `to_json`

Returns validation issues as JSON, including their location, code, and message.

```dream
public fun to_json(): JsonValue
```
