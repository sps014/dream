//! Wall-clock host functions (the `Dream` module behind `src/stdlib/system/datetime.dream` and
//! `system/timezone.dream`). Calendar math itself is implemented in pure Dream; this only bridges
//! the things that genuinely require the host: the current time, the OS's local UTC offset, the
//! IANA timezone database (`chrono-tz`), and the OS's configured IANA zone name
//! (`iana-time-zone`). Browser/Node hosts implement the same names in `runtime/dream.js`.

use std::str::FromStr;
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use wasmtime::*;

use super::memory::{read_arg_string, write_string_to_memory};

static START_INSTANT: OnceLock<Instant> = OnceLock::new();

/// Sentinel `dateZoneOffsetMinutes` returns for a zone name that isn't in the IANA database.
/// Real UTC offsets are always within [-720, 840], so this can never collide with a real one.
pub const UNKNOWN_ZONE_OFFSET: i32 = -999_999;

/// Registers the `DateTime` host functions on `linker`. Shared by the CLI runner and the E2E test
/// harness so the native behavior can never drift.
pub fn link_datetime_functions(linker: &mut Linker<()>) -> Result<()> {
    linker.func_wrap("Dream", "dateNowMillis", || -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    })?;

    linker.func_wrap("Dream", "timeNowNanos", || -> i64 {
        let start = START_INSTANT.get_or_init(Instant::now);
        start.elapsed().as_nanos() as i64
    })?;

    // Minutes *east* of UTC for the local system timezone at the given UTC epoch millisecond
    // instant (e.g. IST is +330, PST is -480), accounting for DST. `runtime/dream.js` mirrors this
    // with the opposite-signed `Date.getTimezoneOffset()`, negated to match this convention.
    linker.func_wrap("Dream", "dateLocalOffsetMinutes", |millis: i64| -> i32 {
        use chrono::{Local, TimeZone};
        match Local.timestamp_millis_opt(millis) {
            chrono::LocalResult::Single(dt) => dt.offset().local_minus_utc() / 60,
            chrono::LocalResult::Ambiguous(dt, _) => dt.offset().local_minus_utc() / 60,
            chrono::LocalResult::None => 0,
        }
    })?;

    // Minutes *east* of UTC for the named IANA zone (e.g. "America/New_York") at the given UTC
    // epoch millisecond instant, accounting for that zone's DST rules at that point in history.
    // Returns `UNKNOWN_ZONE_OFFSET` when `zone_name` isn't a recognized IANA zone identifier.
    linker.func_wrap(
        "Dream",
        "dateZoneOffsetMinutes",
        |mut caller: Caller<'_, ()>, zone_ptr: i32, millis: i64| -> Result<i32> {
            let zone_name = read_arg_string(&mut caller, zone_ptr)?;
            let Ok(tz) = chrono_tz::Tz::from_str(&zone_name) else {
                return Ok(UNKNOWN_ZONE_OFFSET);
            };
            use chrono::{Offset, TimeZone};
            let offset = match tz.timestamp_millis_opt(millis) {
                chrono::LocalResult::Single(dt) => dt.offset().fix().local_minus_utc(),
                chrono::LocalResult::Ambiguous(dt, _) => dt.offset().fix().local_minus_utc(),
                chrono::LocalResult::None => 0,
            };
            Ok(offset / 60)
        },
    )?;

    // The OS's configured IANA timezone name (e.g. "America/New_York"), or "" if it can't be
    // determined (backs `TimeZone.local()`, which falls back to UTC in that case).
    linker.func_wrap(
        "Dream",
        "dateLocalZoneName",
        |mut caller: Caller<'_, ()>| -> Result<i32> {
            let name = iana_time_zone::get_timezone().unwrap_or_default();
            write_string_to_memory(&mut caller, &name)
        },
    )?;

    Ok(())
}
