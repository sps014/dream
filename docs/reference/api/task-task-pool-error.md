# TaskPoolError

**Import:** `import system.task;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `TaskPoolError`

Failures scheduling or closing a reusable worker pool.

```dream
public enum TaskPoolError
```

## `InvalidSize`

The requested worker count was zero or negative.

```dream
InvalidSize
```

## `Closed`

The pool was closed and no longer accepts work.

```dream
Closed
```

## `Cancelled`

Cancellation was requested; any submitted work has settled before this is returned.

```dream
Cancelled
```

## `WorkerCrashed`

The worker running the job stopped before replying: the job trapped, the worker thread exited, a message could not be delivered, or the worker did not start within 10 seconds. The job's result is lost, and objects it held when it stopped stay allocated. The pool replaces the worker, so later dispatches still run. Only the JavaScript hosts report this; on native, a trapping job ends the process.

```dream
WorkerCrashed
```

## `code`

Returns `EINVAL`, `ECLOSED`, `ECANCELED`, or `EWORKER` for the variant.

```dream
public fun code(): string
```

## `message`

Returns a readable description of the failure.

```dream
public fun message(): string
```
