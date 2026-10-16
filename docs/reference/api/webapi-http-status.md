# Http Status

**Import:** `import system.webapi;`

For behavior and native target requirements, see [Native Web APIs](../../web-server/index.md).

## `HttpStatus`

HTTP status used as a handler/dependency short-circuit (`Result<T, HttpStatus>`).

```dream
public class HttpStatus
```

## `code`

HTTP status code.

```dream
public code: int
```

## `message`

Human-readable explanation of the result or failure.

```dream
public message: string
```

## `constructor`

Creates a `HttpStatus` with `code`, `message`.

```dream
public constructor(code: int, message: string = "")
```

## `bad_request`

Creates a 400 Bad Request status.

```dream
public static fun bad_request(): HttpStatus
```

## `unauthorized`

Creates a 401 Unauthorized status.

```dream
public static fun unauthorized(): HttpStatus
```

## `forbidden`

Creates a 403 Forbidden status.

```dream
public static fun forbidden(): HttpStatus
```

## `not_found`

Creates a 404 Not Found response.

```dream
public static fun not_found(): HttpStatus
```

## `conflict`

Creates a 409 Conflict status.

```dream
public static fun conflict(): HttpStatus
```

## `unprocessable`

Creates a 422 Unprocessable Entity status.

```dream
public static fun unprocessable(): HttpStatus
```

## `too_many`

Creates a 429 Too Many Requests status.

```dream
public static fun too_many(): HttpStatus
```

## `internal`

Creates a 500 Internal Server Error status.

```dream
public static fun internal (): HttpStatus
```
