# SharedChannelEndpoints

**Import:** `import system.task;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
