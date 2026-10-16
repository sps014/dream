# Validation Errors

**Import:** `import system.validation;`

See [model validation](../../web-server/validation.md) for strict decoding, constraints, defaults, and callbacks.

## `ValidationIssue`

One input failure; locations use JSON property names and decimal collection indices.

```dream
public class ValidationIssue
```

## `location`

Path to the invalid value, from the outermost field to the failing item.

```dream
public location: List<string>
```

## `code`

Error code identifying the failure.

```dream
public code: string
```

## `message`

Human-readable explanation of the result or failure.

```dream
public message: string
```

## `constructor`

Creates a `ValidationIssue` with `location`, `code`, `message`.

```dream
public constructor(location: List<string>, code: string, message: string)
```

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
