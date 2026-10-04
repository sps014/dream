# TimeZone

**Import:** `import system;`

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct TimeZone`

An IANA timezone identifier (e.g. `"America/New_York"`, `"Europe/London"`), resolving UTC offsets from the host's timezone database — including historical DST rules, unlike a plain fixed offset. Use with `DateTime.now_in`/`DateTime.of_zoned`/`DateTime.to_zone`.

```dream
public struct TimeZone
```

## `name`

The IANA zone identifier (e.g. `"America/New_York"`).

```dream
public get name(): string
```

## `utc`

The UTC zone (offset `0` at every instant).

```dream
public static get utc(): TimeZone
```

## `local`

The host's configured local timezone, or `utc` if it can't be determined.

```dream
public static get local(): TimeZone
```

## `of`

Resolves `name` as an IANA zone identifier (e.g. `"Asia/Kolkata"`), or an error if the host's timezone database doesn't recognize it.

```dream
public static fun of(name: string): Result<TimeZone, ParseError>
```

## `offset_minutes_at`

This zone's UTC offset in minutes at `epoch_millis` (accounting for DST rules in effect at that instant). Falls back to `0` if the zone is no longer recognized by the host.

```dream
public fun offset_minutes_at(epoch_millis: long): int
```
