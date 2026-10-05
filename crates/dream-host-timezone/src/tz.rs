//! IANA timezone offset/name for native `DateTime` hosts.

use chrono::{Offset, TimeZone as _};
use chrono_tz::Tz;

pub(crate) fn zone_offset_minutes(zone: &str, epoch_millis: i64) -> i32 {
    if zone.is_empty() {
        return -999_999;
    }
    if zone == "UTC" {
        return 0;
    }
    let Ok(tz) = zone.parse::<Tz>() else {
        return -999_999;
    };
    match tz.timestamp_millis_opt(epoch_millis) {
        chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => {
            dt.offset().fix().local_minus_utc() / 60
        }
        chrono::LocalResult::None => -999_999,
    }
}

pub(crate) fn local_zone_name() -> String {
    iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string())
}
