# HttpResponse

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
public fun close(): Result<Unit, HttpError>
```
