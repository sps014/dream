# Auth

**Import:** `import system.webapi;`

Read the [usage guide](../stdlib/index.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `BearerToken`

`Authorization: Bearer <token>` → the token, or 401.

```dream
public async fun BearerToken(@header("Authorization") value: string): Result<string, HttpStatus>
```

## `ApiKeyHeader`

Header named `X-Api-Key` (or pass `@header` yourself by wrapping).

```dream
public async fun ApiKeyHeader(@header("X-Api-Key") value: string): Result<string, HttpStatus>
```

## `BasicAuth`

`Authorization: Basic <base64>` → the raw header value after "Basic ".

```dream
public async fun BasicAuth(@header("Authorization") value: string): Result<string, HttpStatus>
```
