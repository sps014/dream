# Duration, DateTime

**Import:** `import system;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](datetime.md)

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

## `DateTime`

A calendar date and time of day.

```dream
public struct DateTime: Equatable<DateTime>
```

## `constructor`

Creates a `DateTime` with `epoch_millis`, `offset_minutes`.

```dream
public constructor(epoch_millis: long, offset_minutes: int)
```

## `epoch_millis`

UTC time as milliseconds since the Unix epoch.

```dream
public get epoch_millis(): long
```

## `offset_minutes`

Offset from UTC, in minutes.

```dream
public get offset_minutes(): int
```

## `utc_now`

The current instant in UTC.

```dream
public static fun utc_now(): DateTime
```

## `now`

The current instant in the local system timezone.

```dream
public static fun now(): DateTime
```

## `now_in`

The current instant in `zone` (an IANA timezone, see `TimeZone`).

```dream
public static fun now_in(zone: TimeZone): DateTime
```

## `from_epoch_millis`

Wraps a raw UTC epoch millisecond instant.

```dream
public static fun from_epoch_millis(millis: long): DateTime
```

## `epoch_seconds`

UTC time as whole seconds since the Unix epoch.

```dream
public get epoch_seconds(): long
```

## `from_epoch_seconds`

Creates a UTC date/time from seconds since the Unix epoch.

```dream
public static fun from_epoch_seconds(seconds: long): DateTime
```

## `of`

Builds a UTC instant from calendar fields.

```dream
public static fun of(
    year: int,
    month: int,
    day: int,
    hour: int,
    minute: int,
    second: int,
    millisecond: int
): DateTime
```

## `of_local`

Builds an instant from calendar fields interpreted as local wall-clock time.

```dream
public static fun of_local(
    year: int,
    month: int,
    day: int,
    hour: int,
    minute: int,
    second: int,
    millisecond: int
): DateTime
```

## `of_zoned`

Builds an instant from calendar fields interpreted as wall-clock time in `zone`.

```dream
public static fun of_zoned(
    year: int,
    month: int,
    day: int,
    hour: int,
    minute: int,
    second: int,
    millisecond: int,
    zone: TimeZone
): DateTime
```

## `to_utc`

The same instant in UTC.

```dream
public fun to_utc(): DateTime
```

## `to_local`

The same instant in the local system timezone.

```dream
public fun to_local(): DateTime
```
