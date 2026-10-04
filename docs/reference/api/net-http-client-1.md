# HttpClient

**Import:** `import system.net;`

Read the [usage guide](../stdlib/http.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](net-http-client.md)

## `class HttpClient`

A cross-runtime HTTP client returning `Result<HttpResponse, HttpError>`.

```dream
public class HttpClient
```

## `constructor`

Creates an HTTP client. `base_url` defaults to empty so absolute URLs work with `HttpClient()`.

```dream
public constructor(base_url: string = "")
```

## `timeout_ms`

Request timeout in milliseconds (`0` = none).

```dream
public get timeout_ms(): int
```

## `timeout_ms`

Sets the request timeout in milliseconds (`0` = none).

```dream
public set timeout_ms(ms: int)
```

## `with_timeout`

Sets the request timeout in milliseconds (`0` = none).

```dream
public fun with_timeout(ms: int): HttpClient
```

## `http_version`

Preferred HTTP version on the native host (`1` = HTTP/1.1, `2` = HTTP/2).

```dream
public get http_version(): int
```

## `http_version`

Sets the preferred HTTP version on the native host (`1` = HTTP/1.1, `2` = HTTP/2).

```dream
public set http_version(version: int)
```

## `with_http_version`

Sets the preferred HTTP version on the native host (`1` = HTTP/1.1, `2` = HTTP/2).

```dream
public fun with_http_version(version: int): HttpClient
```

## `with_cookie_jar`

Attaches a cookie jar used for outbound `Cookie` and inbound `Set-Cookie`.

```dream
public fun with_cookie_jar(jar: CookieJar): HttpClient
```

## `with_cancellation`

Attaches a cooperative cancellation token checked before each request.

```dream
public fun with_cancellation(token: CancellationToken): HttpClient
```

## `set_header`

Adds a default header sent with every request.

```dream
public fun set_header(name: string, value: string): HttpClient
```

## `request`

Performs a request with a text body.

```dream
public async fun request(method: string, path: string, body: string, headers: HttpHeaders, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `get`

Performs a GET request.

```dream
public async fun get(path: string, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `get_with`

```dream
public async fun get_with(path: string, headers: HttpHeaders, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `get_json`

```dream
public async fun get_json(path: string, token: Option<CancellationToken> = Option.None): Result<JsonValue, HttpError>
```

## `post_json`

```dream
public async fun post_json(path: string, body: JsonValue, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `post_form`

POSTs `form` percent-encoded with `Content-Type: application/x-www-form-urlencoded`.

```dream
public async fun post_form(path: string, form: Map<string, string>, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `post`

Performs a POST request.

```dream
public async fun post(path: string, body: string, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `post_with`

```dream
public async fun post_with(path: string, body: string, headers: HttpHeaders, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `put`

Performs a PUT request.

```dream
public async fun put(path: string, body: string, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `put_with`

```dream
public async fun put_with(path: string, body: string, headers: HttpHeaders, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `put_json`

PUTs a JSON body with `Content-Type: application/json`.

```dream
public async fun put_json(path: string, body: JsonValue, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `patch`

Performs a PATCH request.

```dream
public async fun patch(path: string, body: string, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `patch_with`

```dream
public async fun patch_with(path: string, body: string, headers: HttpHeaders, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```
