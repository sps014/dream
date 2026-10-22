# Http Incoming

**Import:** `import system.webapi;`

For behavior and native target requirements, see [Native Web APIs](../../web-server/index.md).

## `HttpIncoming`

Headers and routing fields for one owned inbound HTTP request.

```dream
public class HttpIncoming
```

## `req_id`

Identifier of the active HTTP request.

```dream
public req_id: int
```

## `method`

HTTP request method.

```dream
public method: string
```

## `path`

Request or resource path.

```dream
public path: string
```

## `query`

Query string supplied with the request.

```dream
public query: string
```

## `headers`

HTTP headers associated with this message.

```dream
public headers: HttpHeaders
```

## `client_host`

Address of the client: the connecting peer, or the `X-Forwarded-For` client when the peer is a trusted proxy (see `ServerConfig.forwarded_allow_ips`).

```dream
public client_host: string
```

## `client_port`

Port of the connecting peer, or 0 when `client_host` came from `X-Forwarded-For`.

```dream
public client_port: int
```

## `scheme`

`"http"` or `"https"`: whether the connection used TLS, or the trusted proxy's `X-Forwarded-Proto`.

```dream
public scheme: string
```

## `query_param`

Returns the first decoded query value with this name.

```dream
public fun query_param(name: string): Option<string>
```

## `header`

Returns the first matching request header.

```dream
public fun header(name: string): Option<string>
```

## `cookie`

Returns a named cookie, without decoding its value.

```dream
public fun cookie(name: string): Option<string>
```

## `read_body_bytes`

Reads the whole request body as bytes and caches it; later calls return the same bytes. The server sets no size limit, so bound large bodies at a proxy or read them with `stream`. Fails with `ECLOSED` after `stream` was read.

```dream
public fun read_body_bytes(
    token: Option<CancellationToken> = Option.None
): Future<Result<byte[], IoError>>
```

## `read_body_text`

Reads the whole request body as UTF-8 text, with the caching and limits of `read_body_bytes`.

```dream
public async fun read_body_text(
    token: Option<CancellationToken> = Option.None
): Result<string, IoError>
```

## `stream`

Returns the body as a stream of chunks read as they arrive, for bodies too large to hold in memory. After streaming starts, `read_body_bytes`, `read_body_text` and `form` fail with `ECLOSED`; if the body was already read, the stream replays it.

```dream
public fun stream(): HttpRequestStream
```

## `form`

Parses a `multipart/form-data` body. Text fields larger than `max_part_size` bytes, more than `max_fields` text fields or more than `max_files` files fail with `EINVAL`; file parts have no size limit and spool to disk past 1 MiB. Later calls return the same `FormData`, whatever limits they pass. Fails with `ECLOSED` after `stream` was read.

```dream
public fun form(
    max_files: int = 1000,
    max_fields: int = 1000,
    max_part_size: int = 1048576,
    token: Option<CancellationToken> = Option.None
): Future<Result<FormData, IoError>>
```
