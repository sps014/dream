# HttpError

**Import:** `import system.net;`

Read the [usage guide](../stdlib/http.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class HttpError : Error`

HTTP transport / status failure implementing `Error`.

```dream
public class HttpError : Error
```

## `status: int`

HTTP status when this is a status failure; `0` for transport errors.

```dream
public status: int
```

## `constructor`

Creates an error with the given status, machine code, and message.

```dream
public constructor(status: int, code: string, message: string)
```

## `message`

Human-readable description.

```dream
public fun message(): string
```

## `code`

Stable machine code (`HTTP_0`, `HTTP_404`, …).

```dream
public fun code(): string
```

## `transport`

Transport-level failure (timeout, DNS, connection refused, …); status is `0`.

```dream
public static fun transport(message: string): HttpError
```

## `status`

Non-success HTTP status with a descriptive message.

```dream
public static fun status(status: int, message: string): HttpError
```

## `cancelled`

Cooperative cancellation (`CancellationToken`).

```dream
public static fun cancelled(): HttpError
```
