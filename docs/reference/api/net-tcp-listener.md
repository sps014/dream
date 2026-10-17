# TcpListener

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `TcpListener`

An owned TCP listener; accepted connections retain independent ownership.

```dream
public class TcpListener
```

## `bind`

Binds a listener; port zero selects an ephemeral port reported by local_port.

```dream
public static async fun bind(
    host: string,
    port: int,
    token: Option<CancellationToken> = Option.None
): Result<TcpListener, IoError>
```

## `local_host`

Numeric interface address selected by the host.

```dream
public get local_host(): string
```

## `local_port`

Actual bound port, including an automatically selected ephemeral port.

```dream
public get local_port(): int
```

## `accept`

Accepts a connection with optional deadline and inherited cancellation.

```dream
public async fun accept(
    timeout_ms: int = 0,
    token: Option<CancellationToken> = Option.None
): Result<TcpStream, IoError>
```

## `close`

Closes this listener and cancels pending accepts, preserving accepted streams.

```dream
public fun close(): Result<bool, IoError>
```
