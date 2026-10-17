# ChannelEndpoints

**Import:** `import system.task;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
