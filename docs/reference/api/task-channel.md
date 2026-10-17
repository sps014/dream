# Channel

**Import:** `import system.task;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
