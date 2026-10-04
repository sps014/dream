# Duration, DateTime

**Import:** `import system;`

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](datetime.md)

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

## `day_of_week`

The day of the week (0 = Sunday).

```dream
public get day_of_week(): int
```

## `day_of_year`

The day of the year (1-based).

```dream
public get day_of_year(): int
```

## `add_millis`

Adds milliseconds.

```dream
public fun add_millis(amount: long): DateTime
```

## `add_seconds`

Adds seconds.

```dream
public fun add_seconds(amount: long): DateTime
```

## `add_minutes`

Adds minutes.

```dream
public fun add_minutes(amount: long): DateTime
```

## `add_hours`

Adds hours.

```dream
public fun add_hours(amount: long): DateTime
```

## `add_days`

Adds days.

```dream
public fun add_days(amount: long): DateTime
```

## `add`

```dream
public fun add(d: Duration): DateTime
```

## `until`

```dream
public fun until(other: DateTime): Duration
```

## `start_of_day`

```dream
public fun start_of_day(): DateTime
```

## `compare_to`

Compares to another DateTime.

```dream
public fun compare_to(other: DateTime): int
```

## `is_before`

True if this is before `other`.

```dream
public fun is_before(other: DateTime): bool
```

## `is_after`

True if this is after `other`.

```dream
public fun is_after(other: DateTime): bool
```

## `equals`

True if this equals `other`.

```dream
public fun equals(other: DateTime): bool
```

## `to_iso8601`

Formats as ISO-8601.

```dream
public fun to_iso8601(): string
```
