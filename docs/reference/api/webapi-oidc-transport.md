# Oidc Transport

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `OidcTransport`

Native OIDC exchange transport used by system.webapi.auth; completion obeys cancellation.

```dream
public static class OidcTransport
```

## `exchange`

Performs an OIDC begin or completion request. Honors `token` cancellation and returns an I/O error on failure.

```dream
public static async fun exchange(
    request: JsonValue,
    token: Option<CancellationToken> = Option.None
): Result<JsonValue, IoError>
```
