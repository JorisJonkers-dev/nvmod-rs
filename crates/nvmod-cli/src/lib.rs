// SPDX-License-Identifier: GPL-3.0-only
//! The `nvmod` command line. Every command prints JSON with `--json`, so scripts and the harness never depend
//! on a GUI.

mod args;
mod catalog;
mod doctor;
mod import;
mod init;
mod pe;
mod stage;

use std::io::Write;

/// Exit code for success.
pub const OK: u8 = 0;
/// Exit code for a command that ran and found a problem it was asked to fail on.
pub const FOUND: u8 = 1;
/// Exit code for a command that was refused before it changed anything: an unsupported base copy, an Instance
/// that would be overwritten, or a command line that could not be understood.
pub const REFUSED: u8 = 2;
/// Exit code for a command line that could not be understood (a refusal).
pub const USAGE: u8 = REFUSED;
/// Exit code for a verification that failed (a checksum, an attestation); nothing was installed.
pub const VERIFICATION: u8 = 3;
/// Exit code for a step that failed and was rolled back.
pub const ROLLED_BACK: u8 = 4;

const HELP: &str = "nvmod: install, check and verify the NVSE ecosystem for Fallout: New Vegas / TTW\n\nusage:\n  nvmod pe <file> [--json]    inspect one PE file (a plugin DLL or an executable)\n  nvmod catalog [--json]      list the NVSE plugin catalogue
  nvmod init --base <dir> --instance <dir>
                              make an nvmod Instance skeleton for a Steam or GOG base copy
  nvmod stage --instance <dir> [--profile <name>] [--copy] [--json]
                              build the Staging tree: a hard-link tree of lowercased paths (copies with --copy
                              or where links cannot be made)
  nvmod import-mo2 --mo2 <dir> --instance <dir> --base <dir> [--profile <name>] [--copy] [--json]
                              import one MO2 profile into a new Instance (the MO2 instance is only read)
  nvmod doctor --base <dir> [--json] [--fail-on <severity>]
                              check a base copy of the game (read only)
  nvmod doctor --instance <dir> [--profile <name>] [--game <dir>] [--json] [--fail-on <severity>]
                              check an nvmod Instance, or the NVSE plugins of an MO2 profile (read only)
  nvmod --version | --help

exit codes:
  0  success
  1  problems found
  2  refused (nothing was changed) or a command line that is not understood
  3  verification failure
  4  step failed and rolled back\n";

/// Run the command line `args` (without the program name), writing normal output to `out` and errors to `err`.
/// Returns the process exit code.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    match args.first().map(String::as_str) {
        None | Some("--help" | "-h") => {
            let _ = out.write_all(HELP.as_bytes());
            OK
        }
        Some("pe") => pe::run(&args[1..], out, err),
        Some("catalog") => catalog::run(&args[1..], out, err),
        Some("doctor") => doctor::run(&args[1..], out, err),
        Some("stage") => stage::run(&args[1..], out, err),
        Some("init") => init::run(&args[1..], out, err),
        Some("import-mo2") => import::run(&args[1..], out, err),
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
