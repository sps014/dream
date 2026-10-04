# HttpResponse

**Import:** `import system.net;`

Read the [usage guide](../stdlib/http.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class HttpResponse`

A parsed view over HTTP response bytes.

```dream
public class HttpResponse
```

## `from_wire`

```dream
public static fun from_wire(data: char[]): HttpResponse
```

## `status`

The HTTP status code.

```dream
public get status(): int
```

## `ok`

True for a 2xx status.

```dream
public get ok(): bool
```

## `header`

The value of response header `name`.

```dream
public fun header(name: string): string
```

## `headers`

```dream
public get headers(): HttpHeaders
```

## `text`

The response body as UTF-8 text.

```dream
public fun text(): string
```

## `bytes`

The response body as raw bytes.

```dream
public fun bytes(): byte[]
```

## `json`

The response body parsed as Json.

```dream
public fun json(): Result<JsonValue, ParseError>
```
