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

| Call | Meaning |
| --- | --- |
| `DateTime.now()` | current local time |
| `DateTime.utc_now()` | current UTC |
| `DateTime.of(year, month, day, …)` | build from calendar fields |
| `now_in(tz)` | current time in a [`TimeZone`](#timezone) |
| `of_zoned(..., tz)` / `of_local(...)` | build in a named zone or the machine local zone |
| `from_epoch_millis` / `from_epoch_seconds` | from Unix epoch |

## Read

| Field / call | Meaning |
| --- | --- |
| `.year` / `.month` / `.day` | calendar date |
| `.hour` / `.minute` / `.second` / `.millisecond` | time of day |
| `.day_of_week` | weekday (`0` = Sunday) |
| `.day_of_year` | day of year (1-based) |
| `decompose()` | year/month/day as `DateTimeYmd` |

## Change

| Call | Meaning |
| --- | --- |
| `to_utc()` / `to_local()` / `to_zone(zone)` | convert zone |
| `add_millis` / `add_seconds` / `add_minutes` / `add_hours` / `add_days` | arithmetic |
| `add(d)` / `until(other)` | with a `Duration` |
| `start_of_day()` | midnight of that calendar day |
| `is_before` / `is_after` / `equals` / `compare_to` | compare |
| `==`, `<`, and friends | operators |

## Format and parse

`to_string()` for a default rendering; `to_iso8601()` for ISO text. `parse` (ISO-8601) returns `Result`.

`Duration` is a millisecond span (`from_seconds`, `from_millis`, `as_millis`, `as_seconds`, `add` / `sub`); `DateTime.add(d)` and `until(other)` use it.

## `TimeZone`

Named zones for “what time is it in Tokyo” independent of the machine’s local zone.

## `Time` / `Stopwatch`

`Time.sleep(ms).await` / `delay(ms)` (async, optional last `CancellationToken`). `Time.nano_time()` for a monotonic clock. `Stopwatch` records elapsed time. Also listed under [Built-ins](builtins.md).
