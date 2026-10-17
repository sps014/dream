# UdpDatagram

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `UdpDatagram`

One UDP datagram; empty data is a valid message and never denotes EOF.

```dream
public class UdpDatagram
```

## `data`

Received bytes, limited to the requested receive size.

```dream
public data: byte[]
```

## `host`

Numeric address of the sender.

```dream
public host: string
```

## `port`

UDP port of the sender.

```dream
public port: int
```

## `truncated`

True when the message contained more bytes than the receive buffer could retain.

```dream
public truncated: bool
```
