// SPDX-License-Identifier: GPL-3.0-only
//! Argument handling: positional words, `--flag`, and `--option value`. Small on purpose (see docs/DECISIONS.md).

use std::collections::BTreeMap;

/// Parsed arguments of one command.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Args {
    /// Words that are not options, in order.
    pub positional: Vec<String>,
    flags: Vec<String>,
    options: BTreeMap<String, String>,
}

impl Args {
    /// Parse `args`. `flag_names` take no value; `option_names` take the next word as their value. Anything else
    /// that starts with `--` is an error.
    ///
    /// # Errors
    /// An unknown option, or an option at the end without its value.
    pub fn parse(
        args: &[String],
        flag_names: &[&str],
        option_names: &[&str],
    ) -> Result<Args, String> {
        let mut parsed = Args::default();
        let mut words = args.iter();
        while let Some(word) = words.next() {
            if let Some(name) = word.strip_prefix("--") {
                if flag_names.contains(&name) {
                    parsed.flags.push(name.to_owned());
                } else if option_names.contains(&name) {
                    let value = words
                        .next()
                        .ok_or_else(|| format!("--{name} needs a value"))?;
                    parsed.options.insert(name.to_owned(), value.clone());
                } else {
                    return Err(format!("unknown option --{name}"));
                }
            } else {
                parsed.positional.push(word.clone());
            }
        }
        Ok(parsed)
    }

    /// True when `--name` was given.
    #[must_use]
    pub fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|f| f == name)
    }

    /// The value of `--name`, if given (the last one wins).
    #[must_use]
    #[allow(dead_code)] // used by the doctor command
    pub fn option(&self, name: &str) -> Option<&str> {
        self.options.get(name).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn positional_flags_and_options_are_separated() {
        let a = Args::parse(
            &words("file.dll --json --profile Default"),
            &["json"],
            &["profile"],
        )
        .unwrap();
        assert_eq!(a.positional, ["file.dll"]);
        assert!(a.flag("json") && !a.flag("other"));
        assert_eq!(a.option("profile"), Some("Default"));
        assert_eq!(a.option("game"), None);
    }

    #[test]
    fn unknown_options_and_missing_values_are_errors() {
        assert_eq!(
            Args::parse(&words("--nope"), &[], &[]),
            Err("unknown option --nope".to_owned())
        );
        assert_eq!(
            Args::parse(&words("--profile"), &[], &["profile"]),
            Err("--profile needs a value".to_owned())
        );
    }
}
