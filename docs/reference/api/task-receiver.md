# Receiver

**Import:** `import system.task;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Receiver`

An independently closable consumer endpoint for a local async channel.

```dream
public class Receiver<T>
```

## `try_receive`

Returns a queued item, drained EOF, or an explicit Empty/Closed error without suspension.

```dream
public fun try_receive(): Result<Option<T>, ChannelError>
```

## `receive`

Waits for an item; None means that all senders closed and the queue has drained.

```dream
public fun receive(
    token: Option<CancellationToken> = Option.None
): Future<Result<Option<T>, ChannelError>>
```

## `close`

Closes this endpoint; dropping the last receiver rejects and wakes pending sends.

```dream
public fun close(): void
```
