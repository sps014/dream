# HttpClient

**Import:** `import system.net;`

Read the [usage guide](../stdlib/http.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](net-http-client.md)

## `patch_json`

PATCHes with a JSON body and `Content-Type: application/json`.

```dream
public async fun patch_json(path: string, body: JsonValue, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `delete`

Performs a DELETE request.

```dream
public async fun delete(path: string, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `delete_with`

```dream
public async fun delete_with(path: string, headers: HttpHeaders, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `head`

Performs a HEAD request.

```dream
public async fun head(path: string, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `request_stream`

Performs a request with a text body, streaming the response instead of buffering it.

```dream
public async fun request_stream(method: string, path: string, body: string, headers: HttpHeaders, token: Option<CancellationToken> = Option.None): Result<HttpStreamResponse, HttpError>
```

## `get_stream`

Performs a GET request, streaming the response instead of buffering it.

```dream
public async fun get_stream(path: string, token: Option<CancellationToken> = Option.None): Result<HttpStreamResponse, HttpError>
```

## `request_stream_bytes`

Performs a request with a binary body, streaming the response instead of buffering it.

```dream
public async fun request_stream_bytes(method: string, path: string, body: byte[], headers: HttpHeaders, token: Option<CancellationToken> = Option.None): Result<HttpStreamResponse, HttpError>
```

## `text`

Performs a GET request and resolves with its body as text.

```dream
public async fun text(path: string, token: Option<CancellationToken> = Option.None): Result<string, HttpError>
```

## `get_bytes`

Performs a GET request and resolves with its body as bytes.

```dream
public async fun get_bytes(path: string, token: Option<CancellationToken> = Option.None): Result<byte[], HttpError>
```

## `request_bytes`

Performs a request with a binary body.

```dream
public async fun request_bytes(method: string, path: string, body: byte[], headers: HttpHeaders, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `post_bytes`

Performs a POST request with a binary body.

```dream
public async fun post_bytes(path: string, body: byte[], token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `put_bytes`

Performs a PUT request with a binary body.

```dream
public async fun put_bytes(path: string, body: byte[], token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```

## `post_multipart`

POSTs a multipart form via `request_bytes`.

```dream
public async fun post_multipart(path: string, form: MultipartForm, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError>
```
