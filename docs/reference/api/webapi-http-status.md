# Http Status

**Import:** `import system.webapi;`

For behavior and native target requirements, see [Native Web APIs](../../web-apis/index.md).

## `public class HttpStatus`

HTTP status used as a handler/dependency short-circuit (`Result<T, HttpStatus>`).

```dream
public class HttpStatus
```

## `public code: int`

```dream
public code: int
```

## `public message: string`

```dream
public message: string
```

## `public constructor`

```dream
public constructor(code: int, message: string = "")
```

## `public static fun bad_request`

```dream
public static fun bad_request(): HttpStatus
```

## `public static fun unauthorized`

```dream
public static fun unauthorized(): HttpStatus
```

## `public static fun forbidden`

```dream
public static fun forbidden(): HttpStatus
```

## `public static fun not_found`

```dream
public static fun not_found(): HttpStatus
```

## `public static fun conflict`

```dream
public static fun conflict(): HttpStatus
```

## `public static fun unprocessable`

```dream
public static fun unprocessable(): HttpStatus
```

## `public static fun too_many`

```dream
public static fun too_many(): HttpStatus
```

## `public static fun internal`

```dream
public static fun internal(): HttpStatus
```
