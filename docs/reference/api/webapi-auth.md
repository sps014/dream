# Auth

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `BearerToken`

`Authorization: Bearer <token>` → the token, or 401.

```dream
public async fun BearerToken(@header("Authorization")value: string): Result<string, HttpStatus>
```

## `ApiKeyHeader`

Header named `X-Api-Key` (or pass `@header` yourself by wrapping).

```dream
public async fun ApiKeyHeader(@header("X-Api-Key")value: string): Result<string, HttpStatus>
```

## `BasicAuth`

`Authorization: Basic <base64>` → the raw header value after "Basic ".

```dream
public async fun BasicAuth(@header("Authorization")value: string): Result<string, HttpStatus>
```
