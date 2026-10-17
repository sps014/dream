# TcpStream

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `TcpStream`

An owned TCP byte stream supporting one simultaneous read and one simultaneous write.

```dream
public class TcpStream: AsyncReader, AsyncWriter
```

## `connect`

Connects with a deadline in milliseconds; zero disables the deadline.

```dream
public static async fun connect(
    host: string,
    port: int,
    timeout_ms: int = 0,
    token: Option<CancellationToken> = Option.None
): Result<TcpStream, IoError>
```

## `local_host`

Numeric local interface address selected for this connection.

```dream
public get local_host(): string
```

## `local_port`

Actual bound local port.

```dream
public get local_port(): int
```

## `peer_host`

Numeric remote address of the peer.

```dream
public get peer_host(): string
```

## `peer_port`

Remote TCP port of the peer.

```dream
public get peer_port(): int
```

## `read_async`

Reads up to 16 MiB; positive-size empty success denotes peer EOF.

```dream
public async unique fun read_async(
    max_bytes: int,
    token: Option<CancellationToken> = Option.None
): Result<byte[], IoError>
```

## `read_with_timeout`

Reads with a per-operation deadline; a concurrent read returns EBUSY.

```dream
public fun read_with_timeout(
    max_bytes: int,
    timeout_ms: int,
    token: Option<CancellationToken> = Option.None
): Future<Result<byte[], IoError>>
```

## `write_async`

Writes a partial byte count; cancellation preserves completed side effects.

```dream
public async unique fun write_async(
    bytes: byte[],
    token: Option<CancellationToken> = Option.None
): Result<int, IoError>
```

## `write_all_async`

Retries partial writes until every byte has been accepted; does not roll back failures.

```dream
public async fun write_all_async(
    bytes: byte[],
    token: Option<CancellationToken> = Option.None
): Result<bool, IoError>
```

## `flush_async`

TCP writes are unbuffered by this wrapper; validates openness and cancellation.

```dream
public async unique fun flush_async(
    token: Option<CancellationToken> = Option.None
): Result<bool, IoError>
```

## `shutdown_write`

Sends FIN after accepted writes while retaining the read side; pending writes return EBUSY.

```dream
public fun shutdown_write(): Result<bool, IoError>
```

## `set_nodelay`

Enables or disables TCP_NODELAY; pending native reads may return EBUSY.

```dream
public fun set_nodelay(enabled: bool): Result<bool, IoError>
```

## `close`

Closes the owned socket and wakes pending operations; repeated closure succeeds.

```dream
public fun close(): Result<bool, IoError>
```
