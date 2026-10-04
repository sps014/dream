# Duration, DateTime

**Import:** `import system;`

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](datetime.md)

## `struct Duration`

A signed span of time stored as milliseconds.

```dream
public struct Duration
```

## `millis: long`

```dream
public millis: long
```

## `constructor`

```dream
public constructor(millis: long)
```

## `from_millis`

```dream
public static fun from_millis(ms: long): Duration
```

## `from_seconds`

```dream
public static fun from_seconds(seconds: long): Duration
```

## `as_millis`

```dream
public fun as_millis(): long
```

## `as_seconds`

```dream
public fun as_seconds(): long
```

## `add`

```dream
public fun add(other: Duration): Duration
```

## `sub`

```dream
public fun sub(other: Duration): Duration
```

## `struct DateTime : Equatable<DateTime>`

A calendar date and time of day.

```dream
public struct DateTime : Equatable<DateTime>
```

## `constructor`

```dream
public constructor(epoch_millis: long, offset_minutes: int)
```

## `epoch_millis`

```dream
public get epoch_millis(): long
```

## `offset_minutes`

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

```dream
public get epoch_seconds(): long
```

## `from_epoch_seconds`

```dream
public static fun from_epoch_seconds(seconds: long): DateTime
```

## `of`

Builds a UTC instant from calendar fields.

```dream
public static fun of(year: int, month: int, day: int, hour: int, minute: int, second: int, millisecond: int): DateTime
```

## `of_local`

Builds an instant from calendar fields interpreted as local wall-clock time.

```dream
public static fun of_local(year: int, month: int, day: int, hour: int, minute: int, second: int, millisecond: int): DateTime
```

## `of_zoned`

Builds an instant from calendar fields interpreted as wall-clock time in `zone`.

```dream
public static fun of_zoned(year: int, month: int, day: int, hour: int, minute: int, second: int, millisecond: int, zone: TimeZone): DateTime
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
