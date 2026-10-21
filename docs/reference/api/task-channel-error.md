# ChannelError

**Import:** `import system.task;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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

A bounded channel was created with a capacity of zero or less.

```dream
InvalidCapacity
```

## `code`

Returns `EFULL`, `EEMPTY`, `ECLOSED`, `ECANCELED`, or `EINVAL` for the variant.

```dream
public fun code(): string
```

## `message`

Returns a readable description of the failure.

```dream
public fun message(): string
```
