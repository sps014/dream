# CancelledError

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class CancelledError : Error`

Error raised when a cooperative cancellation token is already cancelled.

```dream
public class CancelledError : Error
```

## `constructor`

```dream
public constructor(message: string)
```

## `message`

Human-readable description.

```dream
public fun message(): string
```

## `code`

Stable machine code.

```dream
public fun code(): string
```

## `cancelled`

Factory for a standard cancelled error.

```dream
public static fun cancelled(): CancelledError
```
