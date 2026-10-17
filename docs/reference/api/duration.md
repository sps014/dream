# Duration

**Import:** `import system;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Duration`

A signed span of time stored as milliseconds.

```dream
public struct Duration
```

## `millis`

Duration in milliseconds.

```dream
public millis: long
```

## `constructor`

Creates a `Duration` with `millis`.

```dream
public constructor(millis: long)
```

## `from_millis`

Creates a duration from milliseconds.

```dream
public static fun from_millis(ms: long): Duration
```

## `from_seconds`

Creates a duration from seconds.

```dream
public static fun from_seconds(seconds: long): Duration
```

## `as_millis`

Returns the duration in milliseconds.

```dream
public fun as_millis(): long
```

## `as_seconds`

Returns the duration in whole seconds.

```dream
public fun as_seconds(): long
```

## `add`

Returns the sum of this duration and `other`.

```dream
public fun add(other: Duration): Duration
```

## `sub`

Returns this duration minus `other`.

```dream
public fun sub(other: Duration): Duration
```
