# Validation

**Import:** `import system.validation;`

See [model validation](../../web-apis/validation.md) for strict decoding, constraints, defaults, and callbacks.

## `public static class Validation`

Strict decoding and validation derived for @json models.

```dream
public static class Validation
```

## `public static extern fun decode<T>`

```dream
public static extern fun decode<T>(text: string): Result<T, ValidationErrors>
```

## `public static extern fun validate<T>`

```dream
public static extern fun validate<T>(value: T): Result<T, ValidationErrors>
```

## `public static extern fun schema<T>`

```dream
public static extern fun schema<T>(): JsonValue
```

## `public static fun child`

```dream
public static fun child(location: List<string>, key: string): List<string>
```

## `public static fun check`

Validates a JSON tree and applies declared defaults before model construction.

```dream
public static fun check(value: JsonValue, schema: JsonValue): ValidationErrors
```
