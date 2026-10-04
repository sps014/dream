# Cancellation Source

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
