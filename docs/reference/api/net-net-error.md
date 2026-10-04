# NetError

**Import:** `import system.net;`

Read the [usage guide](../stdlib/http.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class NetError : Error`

Raw-socket (`TcpClient`/`WebSocket`) transport failure, implementing `Error`.

```dream
public class NetError : Error
```

## `constructor`

Creates an error with a machine code and message.

```dream
public constructor(code: string, message: string)
```

## `message`

Human-readable description.

```dream
public fun message(): string
```

## `code`

Stable machine code (`ECONNECT`, `EIO`, `ECLOSED`, `EPROTOCOL`, `EUNSUPPORTED`).

```dream
public fun code(): string
```

## `connect_failed`

The connection could not be established (DNS, refused, timeout, TLS, ...).

```dream
public static fun connect_failed(message: string): NetError
```

## `io`

A read/write against an open connection failed.

```dream
public static fun io(message: string): NetError
```

## `closed`

The operation was attempted after the connection was already closed.

```dream
public static fun closed(message: string): NetError
```

## `protocol`

The peer sent malformed/unexpected protocol data.

```dream
public static fun protocol(message: string): NetError
```

## `unsupported`

Not available on the current host (e.g. raw TCP in the browser, `wss://` natively).

```dream
public static fun unsupported(message: string): NetError
```

## `cancelled`

Cooperative cancellation (`CancellationToken`).

```dream
public static fun cancelled(): NetError
```
