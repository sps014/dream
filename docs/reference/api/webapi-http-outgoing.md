# Http Outgoing

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `public class HttpOutgoing`

Outbound HTTP response built by handlers and middleware.

```dream
public class HttpOutgoing
```

## `public status`

```dream
public status: int
```

## `public headers`

```dream
public headers: HttpHeaders
```

## `public body`

```dream
public body: string
```

## `public body_bytes`

```dream
public body_bytes: byte[]
```

## `public constructor`

```dream
public constructor(status: int, body: string = "")
```

## `public fun set_header`

```dream
public fun set_header(name: string, value: string): HttpOutgoing
```

## `public static fun text`

```dream
public static fun text(body: string, status: int = 200): HttpOutgoing
```

## `public static fun html`

```dream
public static fun html(body: string, status: int = 200): HttpOutgoing
```

## `public static fun json_text`

```dream
public static fun json_text(body: string, status: int = 200): HttpOutgoing
```

## `public static fun detail`

FastAPI-style `{"detail": "..."}` JSON error.

```dream
public static fun detail(message: string, status: int = 500): HttpOutgoing
```

## `public static fun from_body_error`

Preserves body-limit failures as 413 while other read failures remain 400.

```dream
public static fun from_body_error(error: IoError): HttpOutgoing
```

## `public static fun from_validation`

A structured validation response, prefixed with the input source.

```dream
public static fun from_validation(errors: ValidationErrors, source: string): HttpOutgoing
```

## `public static fun invalid_input`

```dream
public static fun invalid_input(source: string, name: string, code: string, message: string): HttpOutgoing
```

## `public static fun already_sent`

```dream
public static fun already_sent(): HttpOutgoing
```

## `public static fun bytes`

```dream
public static fun bytes(data: byte[], status: int = 200): HttpOutgoing
```

## `public static fun from_status`

```dream
public static fun from_status(st: HttpStatus): HttpOutgoing
```

## `public static fun not_found`

```dream
public static fun not_found(): HttpOutgoing
```

## `public static fun empty`

```dream
public static fun empty(status: int = 204): HttpOutgoing
```
