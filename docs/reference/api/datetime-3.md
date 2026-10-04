# Duration, DateTime

**Import:** `import system;`

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](datetime.md)

## `to_string`

A human-readable formatting.

```dream
public override fun to_string(): string
```

## `parse`

Parses an ISO-8601 datetime (`YYYY-MM-DDTHH:MM:SS[.frac][Z / ±HH:MM]`).

```dream
public static fun parse(text: string): Result<DateTime, ParseError>
```

## `modified`

`mtime` as a UTC `DateTime`.

```dream
public fun modified(): DateTime
```

## `created`

`ctime` as a UTC `DateTime` (inode change time on Unix, not birth time).

```dream
public fun created(): DateTime
```
