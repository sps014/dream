# SharedSender

**Import:** `import system.task;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `SharedSender`

A shared producer; clone creates an independent endpoint and ordinary aliases share closure.

```dream
shared public class SharedSender<T: shared>
```

## `clone`

Creates another producer; the queue closes after the last producer closes or drops.

```dream
public fun clone(): SharedSender<T>
```

## `try_send`

Attempts one send atomically without blocking the scheduler.

```dream
public fun try_send(value: T): Result<Unit, ChannelError>
```

## `send`

Waits for capacity using cross-worker notifications and inherited cancellation.

```dream
public fun send(
    value: T,
    token: Option<CancellationToken> = Option.None
): Future<Result<Unit, ChannelError>>
```

## `close`

Closes this endpoint atomically and wakes affected producers and consumers.

```dream
public fun close(): void
```
