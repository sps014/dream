# TaskHandle

**Import:** `import system.task;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `TaskHandle`

A child result whose execution is owned by a scope, independent of this handle's lifetime.

```dream
public class TaskHandle<T, E>
```

## `join`

Waits for completion without cancelling or transferring ownership of the child.

```dream
public fun join(): Future<Result<T, TaskFailure<E>>>
```
