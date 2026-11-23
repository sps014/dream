import { isNode } from "../platform.js";

// Sentinel `dateZoneOffsetMinutes` returns for a zone name the host doesn't recognize. Must match
// `UNKNOWN_ZONE_OFFSET` in `src/execution/host/datetime.rs`; real UTC offsets are always within
// [-720, 840], so this can never collide with a real one.
const UNKNOWN_ZONE_OFFSET = -999999;

/**
 * Minutes *east* of UTC for the named IANA zone (e.g. "America/New_York") at `millis`, via the
 * standard `Intl.DateTimeFormat` "format then diff" trick: format `millis` as wall-clock parts in
 * `zoneName`, reinterpret those parts as UTC, and the difference from `millis` is the offset.
 * Returns `UNKNOWN_ZONE_OFFSET` if `zoneName` isn't a recognized IANA zone identifier.
 */
function zoneOffsetMinutes(zoneName, millis) {
  const ms = Number(millis);
  try {
    const dtf = new Intl.DateTimeFormat("en-US", {
      timeZone: zoneName,
      hourCycle: "h23",
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
    const parts = {};
    for (const p of dtf.formatToParts(ms)) parts[p.type] = p.value;
    const year = Number(parts.year);
    const month = Number(parts.month) - 1;
    const day = Number(parts.day);
    const hour = parts.hour === "24" ? 0 : Number(parts.hour);
    const minute = Number(parts.minute);
    const second = Number(parts.second);
    // Date.UTC maps years 0–99 onto 1900–1999. Wall time in year 0099 must stay in year 0099.
    let asUtc = Date.UTC(year, month, day, hour, minute, second);
    if (year >= 0 && year < 100) {
      const adjusted = new Date(asUtc);
      adjusted.setUTCFullYear(year, month, day);
      adjusted.setUTCHours(hour, minute, second, 0);
      asUtc = adjusted.getTime();
    }
    return Math.round((asUtc - ms) / 60000);
  } catch (_) {
    return UNKNOWN_ZONE_OFFSET;
  }
}

/** The host's configured IANA timezone name (e.g. "America/New_York"), or "" if unavailable. */
function localZoneName() {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "";
  } catch (_) {
    return "";
  }
}

const cancellableDelay = Object.assign(ms => {
      let resolve;
      const pending = new Promise(done => { resolve = done; });
      const timer = setTimeout(resolve, Math.max(0, ms | 0));
      pending.cancel = () => { clearTimeout(timer); resolve(); };
      return pending;
    }, { cancelSynchronously: true });

export function makeDatetimeTextHost() {
  return {
    dateNowMillis: () => BigInt(Date.now()),
    dateLocalOffsetMinutes: (millis) => -new Date(Number(millis)).getTimezoneOffset(),
    dateZoneOffsetMinutes: (zoneName, millis) => zoneOffsetMinutes(zoneName, millis),
    dateLocalZoneName: () => localZoneName(),
    timeNowNanos: () => {
      if (isNode) {
        return process.hrtime.bigint();
      } else {
        return BigInt(Math.floor(performance.now() * 1000000));
      }
    },
    delayMs: cancellableDelay,
  };
}
