// SPDX-License-Identifier: GPL-3.0-only
//! The request nvmod hands lodwright-rs and the command line that carries it. The request holds the plugin list
//! in Load order, the data path and the settings, and nothing of nvmod's Instance (`docs/architecture/integration.md`,
//! section 3, rule 1). lodwright-rs v0.1.0 has no `--request` option yet, so [`Request::plan_args`] turns the
//! request into the options its `plan` command does take; the request document itself is the part nvmod owns.

use serde_json::{Value, json};
use std::path::PathBuf;

/// The tool name in the lock.
pub const TOOL: &str = "lodwright-rs";
/// The contract of the `plan` output.
pub const PLAN_CONTRACT: &str = "lodwright-plan";
/// The contract a tool must speak before `lod generate` runs it. No lodwright-rs release speaks it yet.
pub const GENERATE_CONTRACT: &str = "lodwright-generate";
/// The version of the request document.
pub const REQUEST_VERSION: u32 = 1;

/// What `lod plan` asks lodwright-rs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The data folder the plugins are read from.
    pub data: PathBuf,
    /// Plugin file names in Load order, masters first.
    pub plugins: Vec<String>,
    /// Plan one worldspace (editor id) instead of all.
    pub world: Option<String>,
    /// Ask for every block, not only the per-level counts.
    pub blocks: bool,
}

impl Request {
    /// The request as a JSON document. It has no Instance type, no Instance path and no profile.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": REQUEST_VERSION,
            "command": "plan",
            "data": self.data.display().to_string(),
            "plugins": self.plugins,
            "settings": { "world": self.world, "blocks": self.blocks },
        })
    }

    /// The arguments of `lodwright plan` that carry this request, one word each (there is no shell).
    #[must_use]
    pub fn plan_args(&self) -> Vec<String> {
        let mut words = vec![
            "plan".to_owned(),
            "--data".to_owned(),
            self.data.display().to_string(),
            "--format".to_owned(),
            "json".to_owned(),
        ];
        for plugin in &self.plugins {
            words.extend(["--plugin".to_owned(), plugin.clone()]);
        }
        if let Some(world) = &self.world {
            words.extend(["--world".to_owned(), world.clone()]);
        }
        if self.blocks {
            words.push("--blocks".to_owned());
        }
        words
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Request {
        Request {
            data: PathBuf::from("/d"),
            plugins: vec!["FalloutNV.esm".to_owned(), "A B.esp".to_owned()],
            world: Some("Testland".to_owned()),
            blocks: true,
        }
    }

    #[test]
    fn the_request_names_no_instance() {
        let doc = request().to_json();
        let keys: Vec<&str> = doc
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            ["command", "data", "plugins", "schema_version", "settings"]
        );
        assert_eq!(doc["plugins"][1], "A B.esp");
    }

    #[test]
    fn the_arguments_keep_the_order_and_one_word_each() {
        assert_eq!(
            request().plan_args(),
            [
                "plan",
                "--data",
                "/d",
                "--format",
                "json",
                "--plugin",
                "FalloutNV.esm",
                "--plugin",
                "A B.esp",
                "--world",
                "Testland",
                "--blocks"
            ]
        );
        let plain = Request {
            world: None,
            blocks: false,
            ..request()
        };
        assert!(!plain.plan_args().contains(&"--blocks".to_owned()));
        assert!(!plain.plan_args().contains(&"--world".to_owned()));
    }
}
