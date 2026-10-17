# DateTime

**Import:** `import system;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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

## `to_zone`

The same instant in `zone` (an IANA timezone, see `TimeZone`).

```dream
public fun to_zone(zone: TimeZone): DateTime
```

## `year`

The year.

```dream
public get year(): int
```

## `month`

The month.

```dream
public get month(): int
```

## `day`

The day.

```dream
public get day(): int
```

## `decompose`

Calendar date fields (year/month/day) computed once.

```dream
public fun decompose(): DateTimeYmd
```

## `hour`

The hour.

```dream
public get hour(): int
```

## `minute`

The minute.

```dream
public get minute(): int
```

## `second`

The second.

```dream
public get second(): int
```

## `millisecond`

The millisecond.

```dream
public get millisecond(): int
```
