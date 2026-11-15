# HttpClient

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `HttpClient`

Owns reusable HTTP connection pools. Close explicitly to cancel pending sends and release pools. Received responses own their bodies independently and must also be closed.

```dream
public class HttpClient
```

## `constructor`

Creates a client with automatic cookies on native hosts by default; pass false to disable them. `cookies` seeds or shares a jar. Without a supplied jar, Node/browser/WASI clients remain stateless. Supplying a jar on an unsupported host makes send return ENOTSUP. Allocation failures surface from send.

```dream
public constructor(use_cookies: bool = true, cookies: Option<CookieJar> = Option.None)
```

## `cookies`

The client jar, or None when automatic cookies are disabled or unavailable and no jar was supplied. A supplied jar remains accessible when use_cookies is false, but is not used for requests or responses.

```dream
public get cookies(): Option<CookieJar>
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
public fun close(): Result<Unit, HttpError>
```
