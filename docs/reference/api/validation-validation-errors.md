# Validation Errors

**Import:** `import system.validation;`

See [model validation](../../web-apis/validation.md) for strict decoding, constraints, defaults, and callbacks.

## `public class ValidationIssue`

One input failure; locations use JSON property names and decimal collection indices.

```dream
public class ValidationIssue
```

## `public location`

```dream
public location: List<string>
```

## `public code`

```dream
public code: string
```

## `public message`

```dream
public message: string
```

## `public constructor`

```dream
public constructor(location: List<string>, code: string, message: string)
```

## `public class ValidationErrors`

Deterministically ordered input errors without echoing sensitive submitted values.

```dream
public class ValidationErrors
```

## `public issues`

```dream
public issues: List<ValidationIssue>
```

## `public constructor`

```dream
public constructor()
```

## `public fun add`

```dream
public fun add(location: List<string>, code: string, message: string): void
```

## `public fun is_empty`

```dream
public fun is_empty(): bool
```

## `public fun to_json`

```dream
public fun to_json(): JsonValue
```
