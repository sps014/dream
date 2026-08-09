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
    const asUtc = Date.UTC(
      Number(parts.year),
      Number(parts.month) - 1,
      Number(parts.day),
      parts.hour === "24" ? 0 : Number(parts.hour),
      Number(parts.minute),
      Number(parts.second),
    );
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

export function makeDatetimeTextHost() {
  return {
    unicodeNormalize: (text, form) => {
      const forms = ["NFC", "NFD", "NFKC", "NFKD"];
      const f = forms[form] || "NFC";
      return String(text).normalize(f);
    },
    unicodeToLower: (text) => String(text).toLocaleLowerCase(),
    unicodeToUpper: (text) => String(text).toLocaleUpperCase(),
    unicodeGraphemes: (text) => {
      const s = String(text);
      if (typeof Intl !== "undefined" && typeof Intl.Segmenter === "function") {
        const seg = new Intl.Segmenter(undefined, { granularity: "grapheme" });
        return Array.from(seg.segment(s), (part) => part.segment);
      }
      return Array.from(s);
    },
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
    delayMs: (ms) => new Promise((resolve) => setTimeout(resolve, Math.max(0, ms | 0))),
  };
}
