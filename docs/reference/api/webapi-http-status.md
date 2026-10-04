# HttpStatus

**Import:** `import system.webapi;`

Read the [usage guide](../stdlib/index.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class HttpStatus`

HTTP status used as a handler/dependency short-circuit (`Result<T, HttpStatus>`).

```dream
public class HttpStatus
```

## `code: int`

```dream
public code: int
```

## `message: string`

```dream
public message: string
```

## `constructor`

```dream
public constructor(code: int, message: string = "")
```

## `bad_request`

```dream
public static fun bad_request(): HttpStatus
```

## `unauthorized`

```dream
public static fun unauthorized(): HttpStatus
```

## `forbidden`

```dream
public static fun forbidden(): HttpStatus
```

## `not_found`

```dream
public static fun not_found(): HttpStatus
```

## `conflict`

```dream
public static fun conflict(): HttpStatus
```

## `unprocessable`

```dream
public static fun unprocessable(): HttpStatus
```

## `too_many`

```dream
public static fun too_many(): HttpStatus
```

## `internal`

```dream
public static fun internal(): HttpStatus
```
