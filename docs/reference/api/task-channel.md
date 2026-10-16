# Channel

**Import:** `import system.task;`

Read the [structured concurrency guide](../../language/concurrency/structured.md) for lifetimes, cancellation, backpressure, and worker execution.

## `ChannelError`

Distinguishes backpressure, exhausted inputs, closed endpoints, and cancellation.

```dream
public enum ChannelError
```

## `Full`

The queue has reached its configured capacity.

```dream
Full
```

## `Empty`

No item is available yet.

```dream
Empty
```

## `Closed`

The relevant endpoint has closed.

```dream
Closed
```

## `Cancelled`

The pending operation was cancelled.

```dream
Cancelled
```

## `InvalidCapacity`

A bounded channel requires a positive capacity.

```dream
InvalidCapacity
```

## `Channel`

Factories for channels with explicit bounded capacity or explicitly unbounded buffering.

```dream
public static class Channel
```

## `shared_bounded`

Creates a bounded cross-worker queue, enforcing the shared transfer constraint.

```dream
public static fun shared_bounded<T: shared>(
    capacity: int
): Result<SharedChannelEndpoints<T>, ChannelError>
```

## `shared_unbounded`

Creates an explicitly unbounded cross-worker queue of shared values.

```dream
public static fun shared_unbounded<T: shared>(): SharedChannelEndpoints<T>
```

## `bounded`

Creates a bounded queue; producers suspend when its explicit capacity is reached.

```dream
public static fun bounded<T>(capacity: int): Result<ChannelEndpoints<T>, ChannelError>
```

## `unbounded`

Creates a queue without backpressure; callers must independently bound production.

```dream
public static fun unbounded<T>(): ChannelEndpoints<T>
```

## `ChannelEndpoints`

A local async queue with independently owned sending and receiving endpoints.

```dream
public class ChannelEndpoints<T>
```

## `sender`

Creates another sending endpoint; dropping the last sender closes sends.

```dream
public get sender(): Sender<T>
```

## `receiver`

Creates another receiving endpoint; dropping the last receiver wakes blocked sends.

```dream
public get receiver(): Receiver<T>
```

## `Sender`

An independently closable producer endpoint for a local async channel.

```dream
public class Sender<T>
```

## `try_send`

Attempts a send without suspension; Full and Closed are distinct errors.

```dream
public fun try_send(value: T): Result<bool, ChannelError>
```

## `send`

Suspends for capacity and observes the inherited scope token unless explicitly overridden.

```dream
public fun send(
    value: T,
    token: Option<CancellationToken> = Option.None
): Future<Result<bool, ChannelError>>
```

## `close`

Closes this endpoint idempotently; the last sender lets receivers drain then finish.

```dream
public fun close(): void
```

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
