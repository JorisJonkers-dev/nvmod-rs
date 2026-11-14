// SPDX-License-Identifier: GPL-3.0-only
//! The `nvmod` command line. Every command prints JSON with `--json`, so scripts and the harness never depend
//! on a GUI.

use std::io::Write;

/// Exit code for success.
pub const OK: u8 = 0;
/// Exit code for a command that ran and found a problem it was asked to fail on.
pub const FOUND: u8 = 1;
/// Exit code for a command line that could not be understood.
pub const USAGE: u8 = 2;

const HELP: &str = "nvmod: install, check and verify the NVSE ecosystem for Fallout: New Vegas / TTW\n\nusage: nvmod [--version] [--help]\n";

/// Run the command line `args` (without the program name), writing normal output to `out` and errors to `err`.
/// Returns the process exit code.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    match args.first().map(String::as_str) {
        None | Some("--help" | "-h") => {
            let _ = out.write_all(HELP.as_bytes());
            OK
        }
        Some("--version" | "-V") => {
            let _ = writeln!(out, "nvmod {}", env!("CARGO_PKG_VERSION"));
            OK
        }
        Some(other) => {
            let _ = writeln!(err, "nvmod: unknown command `{other}`; try --help");
            USAGE
        }
    }
}
