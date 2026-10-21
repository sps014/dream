# HttpAuth

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-server/index.md) for runnable examples, lifecycle, and error behavior.

## `HttpAuth`

Dependencies that read client credentials from standard request headers and document the matching OpenAPI security scheme.

```dream
public static class HttpAuth
```

## `bearer_token`

Extracts the token from an `Authorization: Bearer <token>` header. Use it as a dependency, `@dep(HttpAuth.bearer_token)`. Fails with 401 when the header is missing, uses another scheme, or has an empty token. The token is not verified.

```dream
public static async fun bearer_token(
    @header("Authorization") value: string
): Result<string, HttpStatus>
```

## `api_key_header`

Extracts the key from an `X-Api-Key` header. Use it as a dependency, `@dep(HttpAuth.api_key_header)`. Fails with 401 when the header is missing or empty. The key is not checked.

```dream
public static async fun api_key_header(
    @header("X-Api-Key") value: string
): Result<string, HttpStatus>
```

## `basic_auth`

Extracts the encoded credentials after `Basic ` from an `Authorization` header. Use it as a dependency, `@dep(HttpAuth.basic_auth)`. Fails with 401 when the header is missing, uses another scheme, or is empty. The value is still Base64-encoded and is not verified.

```dream
public static async fun basic_auth(
    @header("Authorization") value: string
): Result<string, HttpStatus>
```
