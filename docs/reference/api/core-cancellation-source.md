# Cancellation Source

**Import:** `import system.core;`

## `CancellationSource`

Cooperative cancellation flag shared with tokens from a `CancellationSource`. `@shared` so a token/source can be captured into a `Task.spawn` body and polled cross-thread.

```dream
shared public class CancellationSource
```

## `constructor`

Creates a source that is not yet cancelled.

```dream
public constructor()
```

## `is_cancelled`

True after `cancel` has been called.

```dream
public get is_cancelled(): bool
```

## `cancel`

Marks this source (and all of its tokens) as cancelled.

```dream
public fun cancel(): void
```

## `token`

Returns a token that observes this source's cancelled state.

```dream
public get token(): CancellationToken
```
