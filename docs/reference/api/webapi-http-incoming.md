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

Reads the bounded request body as bytes, with inherited cancellation.

```dream
public fun read_body_bytes(
    token: Option<CancellationToken> = Option.None
): Future<Result<byte[], IoError>>
```

## `read_body_text`

Reads the bounded request body as UTF-8 text.

```dream
public async fun read_body_text(
    token: Option<CancellationToken> = Option.None
): Result<string, IoError>
```
