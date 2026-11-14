// SPDX-License-Identifier: GPL-3.0-only
//! The `nvmod` command line. Every command prints JSON with `--json`, so scripts and the harness never depend
//! on a GUI.

mod args;
mod catalog;
mod doctor;
mod import;
mod init;
mod install_mod;
mod install_nvrs;
mod pe;
mod sort;
mod stage;

use std::io::Write;

/// Exit code for success.
pub const OK: u8 = 0;
/// Exit code for a command that ran and found a problem it was asked to fail on.
pub const FOUND: u8 = 1;
/// Exit code for a command that was refused before it changed anything: an unsupported base copy or an Instance
/// that would be overwritten.
pub const REFUSED: u8 = 2;
/// Exit code for a command line that could not be understood (unknown command or option, missing argument).
pub const USAGE: u8 = 5;
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
  nvmod install <archive> --instance <dir> [--profile <name>] [--name <mod name>] [--dry-run]
                [--answers <file>] [--extract-tool <path>] [--json]
                              install a Mod from a zip, tar or (through 7z or bsdtar) other archive; a FOMOD is
                              answered where its XML is unambiguous and otherwise exits 1 with the questions
                              (--json), which --answers completes; then stage the profile
  nvmod rollback --instance <dir> [--json]
                              undo the last install or sort: restore the profile files (an install also
                              removes the Mod)
  nvmod sort --instance <dir> [--profile <name>] --masterlist <file> [--apply] [--json]
                              compute the Load order with loot-rs from the Staging tree and print it; with
                              --apply write plugins.txt and loadorder.txt and set the plugin file times
                              (needs `nvmod stage` first)
  nvmod doctor --base <dir> [--json] [--fail-on <severity>]
                              check a base copy of the game (read only)
  nvmod doctor --instance <dir> [--profile <name>] [--game <dir>] [--masterlist <file>] [--catalog <file>]
                [--json] [--fail-on <severity>]
                              check an nvmod Instance (its Staging tree, missing masters, the Load order against
                              loot-rs, NVSE plugins against the catalogue), or the NVSE plugins of an MO2
                              profile (read only)
  nvmod install-nv-rs --instance <dir> [--source <dir>] [--release] [--release-url <url>] [--cargo <path>]
                              [--gh <path>] [--curl <path>] [--play-patch <path>] [--asset <name>] [--repo <o/r>]
                              build nv-rs from source (a Rust toolchain is found through --cargo, NVMOD_CARGO
                              or PATH) or fetch and verify the fork's release; then run the play-patch step
  nvmod --version | --help

exit codes:
  0  success
  1  problems found, or a FOMOD that needs answers
  2  refused (nothing was changed)
  3  verification failure
  4  step failed and rolled back
  5  command line not understood\n";

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
        Some("install") => install_mod::run_install(&args[1..], out, err),
        Some("rollback") => install_mod::run_rollback(&args[1..], out, err),
        Some("sort") => sort::run(&args[1..], out, err),
        Some("install-nv-rs") => install_nvrs::run(&args[1..], out, err),
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
