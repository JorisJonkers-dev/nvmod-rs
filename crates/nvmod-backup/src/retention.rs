// SPDX-License-Identifier: GPL-3.0-only
//! Which snapshots to keep: the newest few, and the newest of each recent day.

use crate::{Config, Error, backups_dir, clock, list};
use nvmod_instance::NvmodInstance;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;

/// The ids to keep out of `snapshots` (`(id, created Unix seconds)`) at the time `now`: the `config.keep_newest`
/// newest, and the newest of each of the last `config.daily_days` UTC days (today counts as the first).
#[must_use]
pub fn select_keep(snapshots: &[(String, i64)], config: &Config, now: i64) -> BTreeSet<String> {
    let mut sorted: Vec<&(String, i64)> = snapshots.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
    let mut keep = BTreeSet::new();
    let mut days = BTreeSet::new();
    let today = clock::day_of(now);
    for (i, (id, time)) in sorted.iter().enumerate() {
        if i < config.keep_newest {
            keep.insert(id.clone());
        }
        let day = clock::day_of(*time);
        if today - day < i64::from(config.daily_days) && days.insert(day) {
            keep.insert(id.clone());
        }
    }
    keep
}

/// What `prune` did (or would do).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PruneReport {
    /// True when nothing was removed.
    pub dry_run: bool,
    /// Ids kept, newest first.
    pub kept: Vec<String>,
    /// Ids removed (or that would be), newest first.
    pub removed: Vec<String>,
}

/// Remove the snapshots of `profile` that `config` does not keep. MO2's own backups and a snapshot whose time
/// cannot be told are never removed.
///
/// # Errors
/// `Failed` when a snapshot folder cannot be removed.
pub fn prune(
    instance: &NvmodInstance,
    profile: &str,
    config: &Config,
    now: i64,
    dry_run: bool,
) -> Result<PruneReport, Error> {
    let all: Vec<(String, i64)> = list(instance, profile)
        .into_iter()
        .filter(|s| s.source == "nvmod" && s.created_unix != 0)
        .map(|s| (s.id, s.created_unix))
        .collect();
    let keep = select_keep(&all, config, now);
    let mut report = PruneReport {
        dry_run,
        kept: Vec::new(),
        removed: Vec::new(),
    };
    for (id, _) in &all {
        if keep.contains(id) {
            report.kept.push(id.clone());
        } else {
            if !dry_run {
                let dir = backups_dir(instance, profile).join(id);
                fs::remove_dir_all(&dir).map_err(|e| Error::Failed(crate::io(&dir, &e)))?;
            }
            report.removed.push(id.clone());
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::{covers, ids};

    /// 40 snapshots twelve hours apart, the newest at 2026-11-14 12:00 UTC (the fake clock's "now"). With 12 hour
    /// spacing the snapshots are at 12:00 (even index) and 00:00 (odd index) of each day, so the newest of day
    /// `j` back is index `2j`.
    #[test]
    fn forty_snapshots_keep_the_newest_five_and_one_per_day_for_fourteen_days() {
        covers(&[ids::safety::backup::RETENTION]);
        let now = clock::parse_utc("20261114T120000Z").unwrap();
        let series: Vec<(String, i64)> = (0..40)
            .map(|k| {
                let t = now - k * 12 * 3600;
                (clock::format_utc(t), t)
            })
            .collect();
        let config = Config {
            interval_secs: 3600,
            keep_newest: 5,
            daily_days: 14,
        };
        let keep = select_keep(&series, &config, now);
        // Newest five: indexes 0..=4. One per day, today and 13 days back: the even indexes 0..=26.
        let mut expected: BTreeSet<usize> = (0..=4).collect();
        expected.extend((0..14).map(|j| 2 * j));
        let expected: BTreeSet<String> = expected.iter().map(|&k| series[k].0.clone()).collect();
        assert_eq!(keep, expected);
        assert_eq!(keep.len(), 16);
        // Nothing older than the 14th day back survives except through the newest-N rule (it does not here).
        assert!(!keep.contains(&series[39].0));
    }

    #[test]
    fn keep_newest_zero_and_no_days_keeps_nothing_and_a_short_series_keeps_all() {
        let now = 1_700_000_000;
        let series = vec![("a".to_owned(), now - 10), ("b".to_owned(), now - 20)];
        let none = Config {
            interval_secs: 0,
            keep_newest: 0,
            daily_days: 0,
        };
        assert!(select_keep(&series, &none, now).is_empty());
        assert_eq!(select_keep(&series, &Config::default(), now).len(), 2);
    }
}
