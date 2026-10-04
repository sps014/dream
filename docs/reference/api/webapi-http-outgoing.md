# HttpOutgoing

**Import:** `import system.webapi;`

Read the [usage guide](../stdlib/index.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class HttpOutgoing`

Outbound HTTP response built by handlers and middleware.

```dream
public class HttpOutgoing
```

## `status: int`

```dream
public status: int
```

## `headers: HttpHeaders`

```dream
public headers: HttpHeaders
```

## `body: string`

```dream
public body: string
```

## `body_bytes: byte[]`

```dream
public body_bytes: byte[]
```

## `constructor`

```dream
public constructor(status: int, body: string = "")
```

## `set_header`

```dream
public fun set_header(name: string, value: string): HttpOutgoing
```

## `text`

```dream
public static fun text(body: string, status: int = 200): HttpOutgoing
```

## `html`

```dream
public static fun html(body: string, status: int = 200): HttpOutgoing
```

## `json_text`

```dream
public static fun json_text(body: string, status: int = 200): HttpOutgoing
```

## `detail`

FastAPI-style `{"detail": "..."}` JSON error.

```dream
public static fun detail(message: string, status: int = 500): HttpOutgoing
```

## `already_sent`

```dream
public static fun already_sent(): HttpOutgoing
```

## `bytes`

```dream
public static fun bytes(data: byte[], status: int = 200): HttpOutgoing
```

## `from_status`

```dream
public static fun from_status(st: HttpStatus): HttpOutgoing
```

## `not_found`

```dream
public static fun not_found(): HttpOutgoing
```

## `empty`

```dream
public static fun empty(status: int = 204): HttpOutgoing
```
