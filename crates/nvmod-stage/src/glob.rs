// SPDX-License-Identifier: GPL-3.0-only
//! Globs over `/` separated paths, ignoring case, as `nvmod overwrite` and the protected paths of the sweep use them.

/// A glob over `/` separated paths, ignoring case: `*` and `?` stay inside one name, `**` crosses folders.
#[must_use]
pub fn glob_match(pattern: &str, path: &str) -> bool {
    fn go(p: &[char], s: &[char]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some('*') if p.get(1) == Some(&'*') => {
                let rest = &p[2..];
                let rest = if rest.first() == Some(&'/') {
                    &rest[1..]
                } else {
                    rest
                };
                (0..=s.len()).any(|i| go(rest, &s[i..]))
            }
            Some('*') => {
                let rest = &p[1..];
                let stop = s.iter().position(|c| *c == '/').unwrap_or(s.len());
                (0..=stop).any(|i| go(rest, &s[i..]))
            }
            Some('?') => s.first().is_some_and(|c| *c != '/') && go(&p[1..], &s[1..]),
            Some(c) => {
                s.first()
                    .is_some_and(|d| d.to_lowercase().eq(c.to_lowercase()))
                    && go(&p[1..], &s[1..])
            }
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = path.chars().collect();
    go(&p, &s)
}

/// Whether `pattern` selects `path`: it matches the path, or a folder the path is under.
#[must_use]
pub fn selects(pattern: &str, path: &str) -> bool {
    let mut at = 0;
    loop {
        let end = path[at..].find('/').map_or(path.len(), |i| at + i);
        if glob_match(pattern, &path[..end]) {
            return true;
        }
        if end == path.len() {
            return false;
        }
        at = end + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_ignore_case_and_stars_stay_in_a_name_unless_doubled() {
        assert!(glob_match("*.log", "Run.LOG"));
        assert!(!glob_match("*.log", "a/run.log"));
        assert!(glob_match("**/*.log", "a/b/run.log"));
        assert!(glob_match("a/?.ini", "A/x.ini"));
        assert!(!glob_match("a/?.ini", "a/xy.ini"));
    }

    #[test]
    fn a_pattern_that_names_a_folder_takes_everything_under_it() {
        assert!(selects("Logs", "logs/a/b.txt"));
        assert!(selects("logs/*", "logs/a/b.txt"));
        assert!(!selects("logs", "other/logs.txt"));
    }
}
