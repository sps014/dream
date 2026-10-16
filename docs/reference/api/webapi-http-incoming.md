# Http Incoming

**Import:** `import system.webapi;`

For behavior and native target requirements, see [Native Web APIs](../stdlib/webapi.md).

## `public class HttpIncoming`

Headers and routing fields for one owned inbound HTTP request.

```dream
public class HttpIncoming
```

## `public req_id: int`

```dream
public req_id: int
```

## `public method: string`

```dream
public method: string
```

## `public path: string`

```dream
public path: string
```

## `public query: string`

```dream
public query: string
```

## `public headers: HttpHeaders`

```dream
public headers: HttpHeaders
```

## `public fun query_param`

Returns the first decoded query value with this name.

```dream
public fun query_param(name: string): Option<string>
```

## `public fun header`

Returns the first matching request header.

```dream
public fun header(name: string): Option<string>
```

## `public fun cookie`

Returns a named cookie, without decoding its value.

```dream
public fun cookie(name: string): Option<string>
```

## `public fun read_body_bytes`

Reads the bounded request body as bytes, with inherited cancellation.

```dream
public fun read_body_bytes(token: Option<CancellationToken> = Option.None): Future<Result<byte[], IoError>>
```

## `public async fun read_body_text`

Reads the bounded request body as UTF-8 text.

```dream
public async fun read_body_text(token: Option<CancellationToken> = Option.None): Result<string, IoError>
```
