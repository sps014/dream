use crate::guest;
use chrono::{Offset, TimeZone};
use std::str::FromStr;
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const UNKNOWN_ZONE_OFFSET: i32 = -999_999;

static START: OnceLock<Instant> = OnceLock::new();

#[no_mangle]
pub extern "C" fn dream_nano_time() -> i64 {
    START.get_or_init(Instant::now).elapsed().as_nanos() as i64
}

#[no_mangle]
pub extern "C" fn dream_now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn dream_date_local_offset_minutes(millis: i64) -> i32 {
    match chrono::Local.timestamp_millis_opt(millis) {
        chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => {
            dt.offset().local_minus_utc() / 60
        }
        chrono::LocalResult::None => 0,
    }
}

#[no_mangle]
pub extern "C" fn dream_date_zone_offset_minutes(zone_ptr: i32, millis: i64) -> i32 {
    let name = guest::read_string(zone_ptr);
    let Ok(tz) = chrono_tz::Tz::from_str(&name) else {
        return UNKNOWN_ZONE_OFFSET;
    };
    let offset = match tz.timestamp_millis_opt(millis) {
        chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => {
            dt.offset().fix().local_minus_utc()
        }
        chrono::LocalResult::None => 0,
    };
    offset / 60
}

#[no_mangle]
pub extern "C" fn dream_date_local_zone_name() -> i32 {
    guest::intern(&iana_time_zone::get_timezone().unwrap_or_default())
}
