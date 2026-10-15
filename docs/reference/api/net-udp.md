# Udp

**Import:** `import system.net;`

## `UdpDatagram`

One UDP datagram; empty data is a valid message and never denotes EOF.

```dream
public class UdpDatagram
```

## `public data: byte[]`

Received bytes, limited to the requested receive size.

```dream
public data: byte[]
```

## `public host: string`

Numeric address of the sender.

```dream
public host: string
```

## `public port: int`

UDP port of the sender.

```dream
public port: int
```

## `public truncated: bool`

True when the message contained more bytes than the receive buffer could retain.

```dream
public truncated: bool
```

## `UdpSocket`

An owned UDP socket with explicit message boundaries and bounded receive buffers.

```dream
public class UdpSocket
```

## `bind`

Binds an endpoint; port zero selects an ephemeral port.

```dream
public static async fun bind(host: string, port: int, token: Option<CancellationToken> = Option.None): Result<UdpSocket, IoError>
```

## `local_host`

Numeric bound interface address.

```dream
public get local_host(): string
```

## `local_port`

Actual bound UDP port.

```dream
public get local_port(): int
```

## `send`

Sends one datagram of at most 65507 bytes with optional deadline and cancellation.

```dream
public async fun send(host: string, port: int, data: byte[], timeout_ms: int = 0, token: Option<CancellationToken> = Option.None): Result<int, IoError>
```

## `receive`

Receives one message, reporting truncation explicitly; a concurrent receive returns EBUSY.

```dream
public async fun receive(max_bytes: int = 65535, timeout_ms: int = 0, token: Option<CancellationToken> = Option.None): Result<UdpDatagram, IoError>
```

## `set_broadcast`

Enables or disables sending to broadcast endpoints.

```dream
public fun set_broadcast(enabled: bool): Result<bool, IoError>
```

## `set_ttl`

Sets outgoing IP time-to-live to a value between 1 and 255.

```dream
public fun set_ttl(ttl: int): Result<bool, IoError>
```

## `close`

Closes the owned socket and wakes pending receives; repeated closure succeeds.

```dream
public fun close(): Result<bool, IoError>
```
