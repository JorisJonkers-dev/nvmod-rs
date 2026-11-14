// SPDX-License-Identifier: GPL-3.0-only
//! UTC timestamps without a date library.

use std::time::{SystemTime, UNIX_EPOCH};

/// (year, month, day) of the day `days` after 1970-01-01 (Howard Hinnant's civil-from-days).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// `(YYYY, MM, DD, hh, mm, ss, millis)` of `time` in UTC.
fn parts(time: SystemTime) -> (i64, u32, u32, u64, u64, u64, u32) {
    let since = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs();
    let (y, m, d) = civil(i64::try_from(secs / 86_400).unwrap_or(0));
    let rest = secs % 86_400;
    (
        y,
        m,
        d,
        rest / 3600,
        rest % 3600 / 60,
        rest % 60,
        since.subsec_millis(),
    )
}

/// RFC 3339 UTC with milliseconds, for the `ts` key.
#[must_use]
pub fn now() -> String {
    format_time(SystemTime::now())
}

/// RFC 3339 UTC with milliseconds.
#[must_use]
pub fn format_time(time: SystemTime) -> String {
    let (y, mo, d, h, mi, s, ms) = parts(time);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{ms:03}Z")
}

/// `YYYYMMDDTHHMMSSmmm`, the start of a run id (so run ids sort by age).
#[must_use]
pub fn compact(time: SystemTime) -> String {
    let (y, mo, d, h, mi, s, ms) = parts(time);
    format!("{y:04}{mo:02}{d:02}T{h:02}{mi:02}{s:02}{ms:03}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn known_instants_format_as_utc() {
        assert_eq!(format_time(UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
        let t = UNIX_EPOCH + Duration::from_millis(1_709_164_800_123); // 2024-02-29 00:00:00.123
        assert_eq!(format_time(t), "2024-02-29T00:00:00.123Z");
        assert_eq!(compact(t), "20240229T000000123");
    }
}
