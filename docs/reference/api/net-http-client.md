# Http Client

**Import:** `import system.net;`

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

## `HttpResponse`

Response metadata and a streamed body. Explicit closure releases unfinished bodies.

```dream
public class HttpResponse
```

## `status`

Numeric HTTP status; non-2xx responses are successful transport results.

```dream
public get status(): int
```

## `url`

Final URL after any redirects visible to the host.

```dream
public get url(): Url
```

## `headers`

Response headers visible to the host; browser CORS rules can hide some headers.

```dream
public get headers(): HttpHeaders
```

## `body`

Byte stream owned by this response; a retained body can be read independently.

```dream
public get body(): HttpBody
```

## `read_text`

Reads remaining body text up to `max_bytes` using strict incremental UTF-8 decoding. Limit or decoding failures consume input already read; close the response after any failure.

```dream
public async fun read_text(
    max_bytes: int = 1048576,
    token: Option<CancellationToken> = Option.None
): Result<string, HttpError>
```

## `close`

Closes the body idempotently; subsequent reads return `ECLOSED`.

```dream
public fun close(): Result<bool, HttpError>
```

## `HttpBody`

An HTTP response byte stream. One read may be pending; cancellation can consume bytes.

```dream
public class HttpBody: AsyncReader
```

## `read_async`

Reads up to `max_bytes`; empty success for a positive request is EOF. Zero consumes nothing. Errors use `IoError` for compatibility with async streams; concurrent reads return `EBUSY`.

```dream
public async unique fun read_async(
    max_bytes: int,
    token: Option<CancellationToken> = Option.None
): Result<byte[], IoError>
```

## `close`

Releases the response transport and cancels a pending read; safe to call repeatedly.

```dream
public fun close(): Result<bool, HttpError>
```
