# Shared Channel

**Import:** `import system.task;`

Read the [structured concurrency guide](../../language/concurrency/structured.md) for lifetimes, cancellation, backpressure, and worker execution.

## `SharedChannelEndpoints`

Endpoint factories for a queue that can transfer shared values across workers.

```dream
public class SharedChannelEndpoints<T: shared>
```

## `sender`

Creates an independently closable shared producer endpoint.

```dream
public get sender(): SharedSender<T>
```

## `receiver`

Creates an independently closable shared consumer endpoint.

```dream
public get receiver(): SharedReceiver<T>
```

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
public fun try_send(value: T): Result<bool, ChannelError>
```

## `send`

Waits for capacity using cross-worker notifications and inherited cancellation.

```dream
public fun send(
    value: T,
    token: Option<CancellationToken> = Option.None
): Future<Result<bool, ChannelError>>
```

## `close`

Closes this endpoint atomically and wakes affected producers and consumers.

```dream
public fun close(): void
```

## `SharedReceiver`

A shared consumer; queued values remain available after the last producer closes.

```dream
shared public class SharedReceiver<T: shared>
```

## `clone`

Creates an independently closable consumer on the same queue.

```dream
public fun clone(): SharedReceiver<T>
```

## `try_receive`

Attempts one receive, distinguishing Empty, closed endpoints, and drained EOF.

```dream
public fun try_receive(): Result<Option<T>, ChannelError>
```

## `receive`

Waits for a value or drained EOF using cross-worker notifications.

```dream
public fun receive(
    token: Option<CancellationToken> = Option.None
): Future<Result<Option<T>, ChannelError>>
```

## `close`

Closes this endpoint; the last consumer releases queued values and wakes blocked sends.

```dream
public fun close(): void
```
