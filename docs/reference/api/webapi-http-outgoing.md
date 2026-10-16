# Http Outgoing

**Import:** `import system.webapi;`

For behavior and native target requirements, see [Native Web APIs](../stdlib/webapi.md).

## `public class HttpOutgoing`

Outbound HTTP response built by handlers and middleware.

```dream
public class HttpOutgoing
```

## `public status: int`

```dream
public status: int
```

## `public headers: HttpHeaders`

```dream
public headers: HttpHeaders
```

## `public body: string`

```dream
public body: string
```

## `public body_bytes: byte[]`

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
