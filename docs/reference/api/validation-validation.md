# Validation

**Import:** `import system.validation;`

See [model validation](../../web-server/validation.md) for strict decoding, constraints, defaults, and callbacks.

## `Validation`

Strict decoding and validation derived for @json models.

```dream
public static class Validation
```

## `decode`

Decodes and validates JSON as `T`, returning the collected validation issues on failure.

```dream
public static extern fun decode<T>(text: string): Result<T, ValidationErrors>
```

## `validate`

Checks `value` against its validation attributes; returns the value or collected issues.

```dream
public static extern fun validate<T>(value: T): Result<T, ValidationErrors>
```

## `schema`

Returns the JSON Schema describing `T` and its validation constraints.

```dream
public static extern fun schema<T>(): JsonValue
```

## `child`

Returns a new validation location with `key` appended.

```dream
public static fun child(location: List<string>, key: string): List<string>
```

## `check`

Validates a JSON tree and applies declared defaults before model construction.

```dream
public static fun check(value: JsonValue, schema: JsonValue): ValidationErrors
```
