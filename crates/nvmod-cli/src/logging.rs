// SPDX-License-Identifier: GPL-3.0-only
//! The run log of a command (ADR-0012): `--log-level`, `NVMOD_LOG`, the redaction setup and the session around
//! `run_main`. Library crates only emit `tracing` events; this is the one place that installs a subscriber.

use std::path::Path;

use nvmod_runlog::{Config, Redactor, Session};
use tracing::level_filters::LevelFilter;

use crate::USAGE;

/// The `--option <path>` flags whose value is a folder that gets a named placeholder in the log, and the
/// placeholder (`--instance` is `$INSTANCE`, as the ADR says).
const FOLDERS: [(&str, &str); 7] = [
    ("instance", "$INSTANCE"),
    ("base", "$BASE"),
    ("game", "$GAME"),
    ("fallout3", "$FALLOUT3"),
    ("mo2", "$MO2"),
    ("source", "$SOURCE"),
    ("store", "$STORE"),
];

/// Take `--log-level <level>` (or `--log-level=<level>`) out of `args`; a `--` ends the search, because what
/// follows belongs to a tool. Returns the other arguments and the level, if given.
fn take_level(args: &[String]) -> Result<(Vec<String>, Option<LevelFilter>), String> {
    let mut rest = Vec::with_capacity(args.len());
    let mut level = None;
    let mut words = args.iter();
    while let Some(word) = words.next() {
        if word == "--" {
            rest.push(word.clone());
            rest.extend(words.by_ref().cloned());
            break;
        }
        let value = if word == "--log-level" {
            Some(words.next().ok_or("--log-level needs a value")?.clone())
        } else {
            word.strip_prefix("--log-level=").map(str::to_owned)
        };
        match value {
            Some(v) => {
                level = Some(v.parse::<LevelFilter>().map_err(|_| {
                    format!("--log-level: `{v}` is not off, error, warn, info, debug or trace")
                })?);
            }
            None => rest.push(word.clone()),
        }
    }
    Ok((rest, level))
}

/// The command's name for the log: its first word, and the subcommand of the commands that have one. Never an
/// option or its value.
fn op_of(args: &[String]) -> String {
    let first = args.first().map_or("nvmod", String::as_str);
    let sub = args.get(1).filter(|w| !w.starts_with('-'));
    match (first, sub) {
        ("mods" | "tools" | "profile", Some(s)) => format!("{first} {s}"),
        _ => first.to_owned(),
    }
}

fn redactor_for(args: &[String]) -> Redactor {
    let mut redactor = Redactor::new().with_home();
    for (name, placeholder) in FOLDERS {
        let flag = format!("--{name}");
        for pair in args.windows(2) {
            if pair[0] == flag {
                redactor = redactor.with_folder(Path::new(&pair[1]), placeholder);
            }
        }
    }
    redactor
}

/// What `begin` needs from the environment (a seam for tests).
fn env_level() -> Option<LevelFilter> {
    std::env::var("NVMOD_LOG").ok().and_then(|v| v.parse().ok())
}

/// Start the run log for the command line `args`. Returns the arguments without `--log-level` and the session;
/// the caller runs the command on the returned arguments and calls [`Session::finish`] with its exit code.
/// `--help`, `--version` and an empty command line write no file. A bad `--log-level` is exit code 5.
///
/// # Errors
/// The usage message when `--log-level` is malformed.
pub fn begin(args: &[String]) -> Result<(Vec<String>, Session), String> {
    let (rest, flag_level) = take_level(args)?;
    let quiet = matches!(
        rest.first().map(String::as_str),
        None | Some("--help" | "-h" | "--version" | "-V")
    );
    let session = Session::start(Config {
        op: op_of(&rest),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        state_dir: None,
        redactor: redactor_for(&rest),
        stderr_level: flag_level.or_else(env_level).unwrap_or(LevelFilter::WARN),
        write_file: !quiet,
    });
    Ok((rest, session))
}

/// The whole life of one process: `begin`, `run_main`, `finish`. Returns the exit code.
pub fn run_logged(
    args: &[String],
    out: &mut dyn std::io::Write,
    err: &mut dyn std::io::Write,
) -> u8 {
    match begin(args) {
        Ok((rest, session)) => {
            let code = crate::run_main(&rest, out, err);
            session.finish(code);
            code
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod: {why}");
            USAGE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn the_level_option_is_taken_out_wherever_it_is() {
        let (rest, level) = take_level(&words("doctor --log-level debug --json")).unwrap();
        assert_eq!(rest, words("doctor --json"));
        assert_eq!(level, Some(LevelFilter::DEBUG));
        let (rest, level) = take_level(&words("pe x.dll --log-level=trace")).unwrap();
        assert_eq!((rest, level), (words("pe x.dll"), Some(LevelFilter::TRACE)));
    }

    #[test]
    fn what_follows_a_double_dash_belongs_to_the_tool() {
        let (rest, level) = take_level(&words("tools run t -- --log-level debug")).unwrap();
        assert_eq!(rest, words("tools run t -- --log-level debug"));
        assert_eq!(level, None);
    }

    #[test]
    fn a_bad_or_missing_level_is_an_error() {
        assert!(take_level(&words("doctor --log-level loud")).is_err());
        assert!(take_level(&words("doctor --log-level")).is_err());
    }

    #[test]
    fn the_op_is_the_command_and_its_subcommand_only() {
        assert_eq!(
            op_of(&words("mods reorder A --top --instance /x")),
            "mods reorder"
        );
        assert_eq!(op_of(&words("install a.zip --instance /x")), "install");
        assert_eq!(op_of(&words("mods --instance /x")), "mods");
        assert_eq!(op_of(&[]), "nvmod");
    }
}
