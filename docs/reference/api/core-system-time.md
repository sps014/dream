# SystemTime

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `SystemTime`

An instant on the UTC timeline, measured in milliseconds since the Unix epoch. Negative values are before 1970-01-01. This is a point in time, not a calendar date: time zones and civil fields live on `DateTime`.

```dream
public struct SystemTime
```

## `constructor`

The instant `millis` milliseconds after the Unix epoch.

```dream
public constructor(millis: long)
```

## `unix_epoch`

1970-01-01T00:00:00Z.

```dream
public static fun unix_epoch(): SystemTime
```

## `now`

The current UTC time. The clock can jump when the operating system adjusts it.

```dream
public static fun now(): SystemTime
```

## `epoch_millis`

Milliseconds since the Unix epoch.

```dream
public get epoch_millis(): long
```

## `add_millis`

This instant shifted by `millis` milliseconds. Negative values move earlier.

```dream
public fun add_millis(millis: long): SystemTime
```
