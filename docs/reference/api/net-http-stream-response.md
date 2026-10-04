# HttpStreamResponse

**Import:** `import system.net;`

Read the [usage guide](../stdlib/http.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class HttpStreamResponse`

A streaming HTTP response body opened by `HttpClient.get_stream`/`request_stream`. The status line and headers are already available (from the initial request); `read_chunk` pulls the body incrementally instead of buffering it all up front, so large downloads don't need to fit in memory at once.

```dream
public class HttpStreamResponse
```

## `status`

The HTTP status code.

```dream
public get status(): int
```

## `ok`

```dream
public get ok(): bool
```

## `header`

The value of response header `name`.

```dream
public fun header(name: string): string
```

## `headers`

```dream
public get headers(): HttpHeaders
```

## `read_chunk`

Reads up to `max_bytes` of the body. `Ok(None)` marks end-of-stream; the actual chunk size is host-determined and may be smaller than `max_bytes` even before EOF.

```dream
public async fun read_chunk(max_bytes: int, token: Option<CancellationToken> = Option.None): Result<Option<byte[]>, HttpError>
```

## `read_all`

Reads and concatenates every remaining chunk into one `byte[]`.

```dream
public async fun read_all(token: Option<CancellationToken> = Option.None): Result<byte[], HttpError>
```

## `close`

Closes the stream early (e.g. after reading only the headers, or on cancellation). Safe to call more than once.

```dream
public fun close(): void
```
