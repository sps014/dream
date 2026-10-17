# HttpClient

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `HttpClient`

Owns reusable HTTP connection pools. Close explicitly to cancel pending sends and release pools. Received responses own their bodies independently and must also be closed.

```dream
public class HttpClient
```

## `constructor`

Creates a client; allocation failures are reported by `send`.

```dream
public constructor()
```

## `send`

Sends with default options: a 30-second total deadline and the host redirect policy.

```dream
public async fun send(
    request: HttpRequest,
    token: Option<CancellationToken> = Option.None
): Result<HttpResponse, HttpError>
```

## `send`

Sends a snapshot and returns after response headers arrive. Statuses such as 404 remain `Ok`. Cancellation aborts pending transport work; response ownership transfers to the caller on success.

```dream
public async fun send(
    request: HttpRequest,
    options: HttpOptions,
    token: Option<CancellationToken> = Option.None
): Result<HttpResponse, HttpError>
```

## `close`

Closes idempotently, cancelling pending sends. Previously received bodies remain usable.

```dream
public fun close(): Result<bool, HttpError>
```
