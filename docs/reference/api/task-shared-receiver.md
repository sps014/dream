# SharedReceiver

**Import:** `import system.task;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
