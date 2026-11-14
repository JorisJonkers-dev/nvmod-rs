// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod pe <file> [--json]`: inspect one PE file.

use crate::{FOUND, OK, USAGE, args::Args};
use nvmod_pe::{PeReport, inspect_file};
use serde::Serialize;
use std::io::Write;
use std::path::Path;

#[derive(Serialize)]
struct Output<'a> {
    file: &'a str,
    report: &'a PeReport,
}

#[derive(Serialize)]
struct Failure<'a> {
    file: &'a str,
    error: String,
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(args, &["json"], &[]) {
        Ok(args) => args,
        Err(why) => {
            let _ = writeln!(err, "nvmod pe: {why}");
            return USAGE;
        }
    };
    let [file] = args.positional.as_slice() else {
        let _ = writeln!(err, "usage: nvmod pe <file> [--json]");
        return USAGE;
    };
    let json = args.flag("json");
    match inspect_file(Path::new(file)) {
        Ok(report) if json => {
            let _ = writeln!(
                out,
                "{}",
                serde_json::to_string_pretty(&Output {
                    file,
                    report: &report
                })
                .unwrap_or_default()
            );
            OK
        }
        Ok(report) => {
            let _ = out.write_all(describe(file, &report).as_bytes());
            OK
        }
        Err(error) if json => {
            let failure = Failure {
                file,
                error: error.to_string(),
            };
            let _ = writeln!(
                out,
                "{}",
                serde_json::to_string_pretty(&failure).unwrap_or_default()
            );
            FOUND
        }
        Err(error) => {
            let _ = writeln!(err, "{file}: {error}");
            FOUND
        }
    }
}

/// The report as text for people.
fn describe(file: &str, report: &PeReport) -> String {
    let yes_no = |on: bool| if on { "yes" } else { "no" };
    let mut lines = vec![
        file.to_owned(),
        format!(
            "  format: {:?} {}{}",
            report.format,
            report.machine_name,
            if report.is_dll { " dll" } else { " exe" }
        ),
        format!(
            "  large address aware: {}",
            yes_no(report.large_address_aware)
        ),
    ];
    if let Some(v) = report.file_version {
        lines.push(format!("  file version: {v}"));
    }
    if let Some(v) = report.product_version {
        lines.push(format!("  product version: {v}"));
    }
    let nvse = &report.nvse;
    if nvse.is_plugin {
        let version = match nvse.version_struct {
            Some(v) => format!("info {} version {}", v.info_version, v.version),
            None if nvse.has_version_export => "exported, unreadable".to_owned(),
            None => "not exported".to_owned(),
        };
        lines.push(format!(
            "  NVSE plugin: query {}, load {}, version struct {version}",
            yes_no(nvse.has_query),
            yes_no(nvse.has_load)
        ));
    } else {
        lines.push("  NVSE plugin: no (none of the three entry points is exported)".to_owned());
    }
    lines.push(format!("  exports: {}", report.exports.len()));
    if !report.imports.is_empty() {
        lines.push(format!("  imports: {}", report.imports.join(", ")));
    }
    lines.extend(
        report
            .issues
            .iter()
            .map(|issue| format!("  issue: {issue}")),
    );
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_pe::{NvseInfo, VersionStruct, inspect_bytes};
    use nvmod_testkit::PeBuilder;

    fn report(builder: &PeBuilder) -> PeReport {
        inspect_bytes(&builder.build()).unwrap()
    }

    #[test]
    fn the_text_names_each_state_of_the_version_struct() {
        let mut r = report(&PeBuilder::nvse_plugin());
        assert!(describe("a.dll", &r).contains("version struct not exported"));
        r.nvse = NvseInfo {
            has_version_export: true,
            ..r.nvse
        };
        assert!(describe("a.dll", &r).contains("version struct exported, unreadable"));
        r.nvse.version_struct = Some(VersionStruct {
            info_version: 1,
            version: 2,
        });
        assert!(describe("a.dll", &r).contains("version struct info 1 version 2"));
    }

    #[test]
    fn the_text_lists_versions_issues_and_the_kind_of_file() {
        let mut r = report(
            &PeBuilder::exe()
                .versions([1, 4, 0, 525], [1, 4, 0, 526])
                .large_address_aware(true),
        );
        r.issues.push("exports: broken".to_owned());
        let text = describe("game.exe", &r);
        for want in [
            "Pe32 i386 exe",
            "large address aware: yes",
            "file version: 1.4.0.525",
            "product version: 1.4.0.526",
            "issue: exports: broken",
        ] {
            assert!(text.contains(want), "{want} not in {text}");
        }
    }
}
