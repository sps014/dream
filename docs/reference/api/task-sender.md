# Sender

**Import:** `import system.task;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Sender`

An independently closable producer endpoint for a local async channel.

```dream
public class Sender<T>
```

## `try_send`

Attempts a send without suspension; Full and Closed are distinct errors.

```dream
public fun try_send(value: T): Result<Unit, ChannelError>
```

## `send`

Suspends for capacity and observes the inherited scope token unless explicitly overridden.

```dream
public fun send(
    value: T,
    token: Option<CancellationToken> = Option.None
): Future<Result<Unit, ChannelError>>
```

## `close`

Closes this endpoint idempotently; the last sender lets receivers drain then finish.

```dream
public fun close(): void
```
