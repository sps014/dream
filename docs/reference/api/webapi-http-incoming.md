# HttpIncoming

**Import:** `import system.webapi;`

Read the [usage guide](../stdlib/index.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class HttpIncoming`

One inbound HTTP request (headers + routing fields). Body is read through `req_id`.

```dream
public class HttpIncoming
```

## `req_id: int`

```dream
public req_id: int
```

## `method: string`

```dream
public method: string
```

## `path: string`

```dream
public path: string
```

## `query: string`

```dream
public query: string
```

## `headers: HttpHeaders`

```dream
public headers: HttpHeaders
```

## `constructor`

```dream
public constructor( req_id: int, method: string, path: string, query: string, headers: HttpHeaders )
```

## `query_param`

```dream
public fun query_param(name: string): Option<string>
```

## `header`

```dream
public fun header(name: string): Option<string>
```

## `cookie`

```dream
public fun cookie(name: string): Option<string>
```

## `read_body_text`

```dream
public fun read_body_text(): string
```

## `read_body_bytes`

```dream
public fun read_body_bytes(): byte[]
```
