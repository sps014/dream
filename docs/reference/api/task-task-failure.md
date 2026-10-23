# Task Failure

**Import:** `import system.task;`

Read the [structured concurrency guide](../../language/concurrency/structured.md) for lifetimes, cancellation, backpressure, and worker execution.

## `TaskFailure`

Preserves application errors while distinguishing structured cancellation and deadlines.

```dream
public enum TaskFailure<E>
```

## `Failed`

A child or the scope body returned an application error.

```dream
Failed(E)
```

## `Cancelled`

The parent or caller requested cancellation.

```dream
Cancelled
```

## `TimedOut`

The scope deadline expired before all work completed.

```dream
TimedOut
```

## `WorkerFailed`

A worker rejected a submitted job or crashed before replying; the error says which.

```dream
WorkerFailed(TaskPoolError)
```

## `InvalidOperation`

A concurrency limit or scope operation was invalid.

```dream
InvalidOperation
```
