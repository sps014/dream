# Duration

**Import:** `import system;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Duration`

A signed span of time with millisecond precision. Add one to a `DateTime` with `DateTime.add`, or combine durations with `add` and `sub`.

```dream
public struct Duration
```

## `millis`

Length of the span in milliseconds; negative values point backward in time.

```dream
public millis: long
```

## `constructor`

Creates a duration of `millis` milliseconds.

```dream
public constructor(millis: long)
```

## `from_seconds`

Creates a duration of `seconds` whole seconds.

```dream
public static fun from_seconds(seconds: long): Duration
```

## `from_minutes`

Creates a duration of `minutes` whole minutes.

```dream
public static fun from_minutes(minutes: long): Duration
```

## `from_hours`

Creates a duration of `hours` whole hours.

```dream
public static fun from_hours(hours: long): Duration
```

## `from_days`

Creates a duration of `days` 24-hour days. Calendar days that cross a daylight-saving change are not adjusted.

```dream
public static fun from_days(days: long): Duration
```

## `as_seconds`

Returns the duration in whole seconds, truncated toward zero.

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
