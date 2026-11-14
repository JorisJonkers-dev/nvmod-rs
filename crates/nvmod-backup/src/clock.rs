// SPDX-License-Identifier: GPL-3.0-only
//! UTC time as whole seconds, with no time zone database: the snapshot id format and the day arithmetic that
//! pruning needs. Callers pass the time in, so tests run on a fake clock.

/// Days from 1970-01-01 to the civil date (proleptic Gregorian).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// `20261114T153012Z` for the Unix time `secs`.
#[must_use]
pub fn format_utc(secs: i64) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let rest = secs.rem_euclid(86_400);
    format!(
        "{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

fn field(text: &str, from: usize, len: usize) -> Option<i64> {
    let part = text.get(from..from + len)?;
    part.bytes()
        .all(|b| b.is_ascii_digit())
        .then(|| part.parse().ok())?
}

/// The Unix time of `20261114T153012Z` (a suffix after the `Z`, as in `...Z-02`, is ignored).
#[must_use]
pub fn parse_utc(text: &str) -> Option<i64> {
    if text.len() < 16 || text.as_bytes()[8] != b'T' || text.as_bytes()[15] != b'Z' {
        return None;
    }
    let (y, mo, d) = (field(text, 0, 4)?, field(text, 4, 2)?, field(text, 6, 2)?);
    let (h, mi, s) = (field(text, 9, 2)?, field(text, 11, 2)?, field(text, 13, 2)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 59 {
        return None;
    }
    Some(days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s)
}

/// The UTC day number (days since 1970-01-01) of `secs`.
#[must_use]
pub fn day_of(secs: i64) -> i64 {
    secs.div_euclid(86_400)
}

/// The current Unix time, or the value of `NVMOD_NOW` (whole seconds) when it is set, which tests use as a clock.
#[must_use]
pub fn now() -> i64 {
    if let Some(fake) = std::env::var("NVMOD_NOW")
        .ok()
        .and_then(|v| v.trim().parse().ok())
    {
        return fake;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_round_trip_and_known_dates_hold() {
        assert_eq!(format_utc(0), "19700101T000000Z");
        assert_eq!(format_utc(1_700_000_000), "20231114T221320Z");
        assert_eq!(parse_utc("20231114T221320Z"), Some(1_700_000_000));
        assert_eq!(parse_utc("20240229T235959Z-02"), Some(1_709_251_199));
        for t in [-86_400, 0, 951_782_400, 4_102_444_800] {
            assert_eq!(parse_utc(&format_utc(t)), Some(t));
        }
        for bad in [
            "",
            "2023",
            "20231314T221320Z",
            "20231114X221320Z",
            "20231114T251320Z",
        ] {
            assert_eq!(parse_utc(bad), None, "{bad}");
        }
        assert_eq!(day_of(86_399), 0);
        assert_eq!(day_of(-1), -1);
    }
}
