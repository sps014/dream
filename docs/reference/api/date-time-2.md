# DateTime

**Import:** `import system;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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

Returns the instant `d` after this date/time, keeping its UTC offset.

```dream
public fun add(d: Duration): DateTime
```

## `until`

Returns the duration from this instant to `other`.

```dream
public fun until(other: DateTime): Duration
```

## `start_of_day`

Returns midnight on the same calendar date, keeping this UTC offset.

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

## `to_string`

A human-readable formatting.

```dream
public override fun to_string(): string
```

## `parse`

Parses an ISO-8601 datetime (`YYYY-MM-DDTHH:MM:SS[.frac][Z|±HH:MM]`).

```dream
public static fun parse(text: string): Result<DateTime, ParseError>
```
