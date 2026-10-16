# Http Outgoing

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-server/index.md) for runnable examples, lifecycle, and error behavior.

## `HttpOutgoing`

Outbound HTTP response built by handlers and middleware.

```dream
public class HttpOutgoing
```

## `status`

HTTP response status code.

```dream
public status: int
```

## `headers`

HTTP headers associated with this message.

```dream
public headers: HttpHeaders
```

## `body`

Text response body.

```dream
public body: string
```

## `body_bytes`

Binary response body; use `bytes` to construct a binary response.

```dream
public body_bytes: byte[]
```

## `constructor`

Creates a `HttpOutgoing` with `status`, `body`.

```dream
public constructor(status: int, body: string = "")
```

## `set_header`

Sets a response header and returns this response for chaining.

```dream
public fun set_header(name: string, value: string): HttpOutgoing
```

## `text`

Creates a plain-text response with status 200 unless another status is supplied.

```dream
public static fun text(body: string, status: int = 200): HttpOutgoing
```

## `html`

Creates an HTML response with status 200 unless another status is supplied.

```dream
public static fun html(body: string, status: int = 200): HttpOutgoing
```

## `json_text`

Creates a JSON response from already encoded JSON text; does not serialize or validate the text.

```dream
public static fun json_text(body: string, status: int = 200): HttpOutgoing
```

## `detail`

FastAPI-style `{"detail": "..."}` JSON error.

```dream
public static fun detail(message: string, status: int = 500): HttpOutgoing
```

## `from_body_error`

Preserves body-limit failures as 413 while other read failures remain 400.

```dream
public static fun from_body_error(error: IoError): HttpOutgoing
```

## `from_validation`

A structured validation response, prefixed with the input source.

```dream
public static fun from_validation(errors: ValidationErrors, source: string): HttpOutgoing
```

## `invalid_input`

Creates a 422 validation-error response describing the invalid input and its location.

```dream
public static fun invalid_input(
    source: string,
    name: string,
    code: string,
    message: string
): HttpOutgoing
```

## `already_sent`

Creates a marker indicating that the response has already been sent.

```dream
public static fun already_sent(): HttpOutgoing
```

## `bytes`

Creates a binary response from `data`, with status 200 by default.

```dream
public static fun bytes(data: byte[], status: int = 200): HttpOutgoing
```

## `from_status`

Creates a response using the supplied status code and message.

```dream
public static fun from_status(st: HttpStatus): HttpOutgoing
```

## `not_found`

Creates a 404 Not Found response.

```dream
public static fun not_found(): HttpOutgoing
```

## `empty`

Creates a response without a body, with status 204 by default.

```dream
public static fun empty(status: int = 204): HttpOutgoing
```
