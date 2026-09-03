# DateTime

**Import:** `import system;`

An instant in time, shown in UTC or a zone offset. Same package as `Time` and `Stopwatch`.

```dream
import system;

fun main() {
    let now = DateTime.now();
    System.println(now.year);
    System.println(now.to_string());
}
```

## Build

`DateTime.now()`, `DateTime.utc_now()`, `DateTime.of(year, month, day, …)`, and `now_in(tz)` / `of_in(..., tz)` when you pass a [`TimeZone`](#timezone).

## Read

Calendar fields such as `.year`, `.month`, `.day`, `.hour`, `.minute`, `.second`, plus helpers for weekday and day-of-year.

## Change

Convert with `.to_utc()` / zone methods. Add or subtract with arithmetic helpers. Compare with `==`, `<`, and friends.

## Format and parse

`to_string()` for a default rendering; `parse` (ISO-8601) returns `Result`. `Duration` is a millisecond span (`from_seconds`, `as_millis`, `add` / `sub`); `DateTime.add(d)` and `until(other)` use it.

## `TimeZone`

Named zones for “what time is it in Tokyo” independent of the machine’s local zone.

## `Time` / `Stopwatch`

`await Time.sleep(ms)` / `delay(ms)` (async, optional last `CancellationToken`). `Time.nano_time()` for a monotonic clock. `Stopwatch` records elapsed time. Also listed under [Built-ins](builtins.md).
