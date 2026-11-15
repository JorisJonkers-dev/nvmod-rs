// SPDX-License-Identifier: GPL-3.0-only
//! The `nvmod` command line. Every command prints JSON with `--json`, so scripts and the harness never depend
//! on a GUI.

mod args;
mod backup;
mod catalog;
mod conflicts;
mod doctor;
mod import;
mod init;
mod install_mod;
mod install_nvrs;
mod lod;
mod logging;
mod mods;
mod mods_life;
mod nvse_usage;
mod pe;
mod plugins;
mod profile;
mod profile_apply;
mod saves;
mod setup;
mod setup_state;
mod sort;
mod stage;
mod target;
mod tools;
mod ttw;
mod verify;

pub use logging::{begin as begin_run_log, run_logged};

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
                              undo the last install, sort or mods change: restore the profile files (an install also
                              removes the Mod)
  nvmod sort --instance <dir> [--profile <name>] (--masterlist <file> | --fetch-masterlist) [--apply] [--json]
                              compute the Load order with loot-rs from the Staging tree and print it; with
                              --apply write plugins.txt and loadorder.txt and set the plugin file times
                              (needs `nvmod stage` first); --fetch-masterlist takes the masterlist pinned in tools.lock.toml
                              (checksum verified, kept in the tool store) instead of a file; locked plugins
                              (`nvmod plugins lock`) keep their position
  nvmod mods list --instance <dir> [--profile <name>] [--category <id>] [--json]
  nvmod mods enable|disable <mod>... --instance <dir> [--profile <name>] [--dry-run] [--json]
  nvmod mods reorder <mod> (--top | --bottom | --before <mod> | --after <mod> | --position <n>)
                --instance <dir> [--profile <name>] [--dry-run] [--json]
                              the Mods of a profile in priority order (1 is the highest, wins a file conflict):
                              list them, enable or disable some, move one; a change edits only modlist.txt,
                              stages the profile again when it was staged, and is undone by `nvmod rollback`
                              (--dry-run writes nothing)
  nvmod mods remove|rename|create|clone|hide|unhide|note <mod> ... --instance <dir> [--dry-run] [--json]
  nvmod mods separator add|rename|remove <label> ... and nvmod mods category set <mod> <id>...
                              the life of a Mod folder and of separators (run `nvmod mods remove` for the
                              forms): journaled and undone by `nvmod rollback`; a removed Mod goes to
                              .nvmod/trash; refused while the game or Mod Organizer runs
  nvmod conflicts --instance <dir> [--profile <name>] [--mod <name>] [--json]
                              the file-level conflicts of a profile (read only): who wins each contested path,
                              and per Mod whether it overwrites, is overwritten, is mixed or redundant
  nvmod tree --instance <dir> [--profile <name>] [--path <prefix>] [--json]
                              the virtual Data folder (read only): which Mod provides each path; the file set
                              of `nvmod stage`
  nvmod backup create|list|prune --instance <dir> [--profile <name>] [--with-saves] [--keep <n>] [--days <n>]
                [--dry-run] [--json]
  nvmod backup restore <id|latest> --instance <dir> [--profile <name>] [--json]
                              snapshots of a profile's lists, INIs and profile.json under .nvmod/backups with a
                              manifest of hashes (real copies; saves only with --with-saves); install, mods and
                              sort --apply take one first when the newest is over an hour old (set it in
                              .nvmod/backups.json); prune keeps the newest 10 and one per day for 14 days;
                              restore verifies (exit 3 on a mismatch), snapshots the current state first and
                              refuses while the game runs; it also reads MO2's own timestamped list backups
  nvmod plugins list --instance <dir> [--profile <name>] [--json]
  nvmod plugins enable <plugin>... [--with-masters] --instance <dir> [--profile <name>] [--dry-run] [--json]
  nvmod plugins disable <plugin>... --instance <dir> [--profile <name>] [--dry-run] [--json]
  nvmod plugins move <plugin> (--top | --bottom | --before <p> | --after <p>) --instance <dir> [--dry-run]
  nvmod plugins lock <plugin> <position> | unlock <plugin> --instance <dir> [--profile <name>] [--dry-run]
                              the plugin list of a profile: list it (position, active, masters present or missing,
                              lock, provider Mod), enable or disable plugins, move one, hold one at a position
                              with `lockedorder.txt`; a write sets plugins.txt and loadorder.txt together and the
                              plugin file times, and is undone by `nvmod rollback`; the forced primary plugins
                              stay first
  nvmod doctor --base <dir> [--json] [--fail-on <severity>]
                              check a base copy of the game (read only)
  nvmod doctor --instance <dir> [--profile <name>] [--game <dir>] [--masterlist <file>] [--catalog <file>]
                [--json] [--fail-on <severity>]
                              check an nvmod Instance (its Staging tree, missing masters, the Load order against
                              loot-rs, NVSE plugins against the catalogue), or the NVSE plugins of an MO2
                              profile (read only)
  nvmod install-nv-rs --instance <dir> [--source <dir>] [--release] [--release-url <url>] [--cargo <path>]
                              [--gh <path>] [--curl <path>] [--asset <name>] [--repo <o/r>]
                              build nv-rs from source (a Rust toolchain is found through --cargo, NVMOD_CARGO
                              or PATH) or fetch and verify the fork's release; then run `nv-viewer <data> --check`
  nvmod ttw --instance <dir> --fallout3 <dir> [--package <file>] [--installer <path>] [--profile <name>] [--json]
                [--lock <file>] [--store <dir>] [--timeout-secs <n>]
                              Tale of Two Wastelands through the pinned external installer (never reimplemented):
                              verify the base game, run the installer with the Fallout 3 folder (it verifies
                              Fallout 3), check its reported result, import the output as the Mod
                              `Tale of Two Wastelands`; a failure exits 4 and changes nothing
  nvmod profile create --instance <dir> [--name <name>]
                              make an empty profile (default name `Default`)
  nvmod profile apply --instance <dir> [--profile <name>] [--prefix <dir>] [--dry-run] [--json]
  nvmod profile finish --instance <dir> [--prefix <dir>] [--dry-run] [--json]
                              place a profile in the Wine prefix before a launch (default prefix: <instance>/compatdata):
                              its game INIs (LocalSettings), a regenerated initweaks.ini, the local-saves redirect,
                              plugins.txt and loadorder.txt, and archive invalidation; whatever it displaces is
                              backed up, a read-only file is refused (exit 2) before any write; finish (and the
                              next apply) copies back what the game changed in the prefix
  nvmod saves list|backup|check|checksum --instance <dir> [--profile <name>] [--prefix <dir>] [--json]
  nvmod saves restore <id|latest> --instance <dir> [--profile <name>] [--prefix <dir>] [--json]
  nvmod saves transfer <from> <to> --instance <dir> [--prefix <dir>] [--force] [--json]
                              the saves of a profile (its saves/ with LocalSaves, else the prefix's My Games Saves):
                              list them with the plugin count from the header (a documented, unverified layout),
                              back up into .nvmod/saves-backup/<UTC>/ with a manifest of hashes, restore a backup
                              (verified, exit 3 on a mismatch; the folder as it was is backed up first; nothing is
                              deleted), copy saves with their .nvse co-saves to another profile (refused without
                              the co-save unless --force, and never over a different file), check each save's
                              plugin list against the profile (exit 1 when one needs a plugin that is not active),
                              or print the checksums that `nvmod run` compares before and after a launch
  nvmod target <vanilla|nv-rs> --instance <dir>
                              choose what the Instance runs on (the Doctor checks xNVSE's DLL on vanilla)
  nvmod setup --base <dir> --instance <dir> --masterlist <file> [--import-mo2 <dir>] [--mod <archive>]...
                [--fallout3 <dir> [--package <file>] [--installer <path>] [--lock <file>] [--store <dir>]]
                [--release | --source <dir>] [--skip-nv-rs] [--profile <name>] [--answers <file>]
                [--keep-partial] [--dry-run] [--json]
  nvmod setup --resume --instance <dir> [--json]
  nvmod setup --restart --instance <dir> --yes
                              base copy to a modded, Doctor-clean Instance in one run: init (or the MO2 import),
                              nv-rs, TTW, each --mod, stage, `sort --apply`, then the Doctor. A failing step
                              removes what the run made and exits with its code, unless --keep-partial leaves
                              the Instance resumable: --resume then continues from the first unfinished step
                              (finished steps are checked again, not trusted) and --restart --yes removes the
                              partial Instance; --dry-run lists the steps and writes nothing; --json prints one
                              event per line (plan, resumed, skipped, step, finished)
  nvmod verify --instance <dir> [--profile <name>] [--masterlist <file>] [--catalog <file>] [--proton <program>]
                [--fail-on <severity>] [--json]
                              confirm an install (read only): the Doctor on the Instance, the installed nv-rs
                              against its install record, and the launch plan of the vanilla plus xNVSE path
                              (a Linux entry that invokes Proton, a Windows entry that runs the game executable
                              directly); exit 3 when verification fails
  nvmod lod plan --instance <dir> [--profile <name>] [--world <name>] [--blocks] [--lodwright <path>]
                [--lock <file>] [--store <dir>] [--timeout-secs <n>] [--json]
  nvmod lod generate --instance <dir> ...
                              plan the LOD blocks of the Load order through the pinned lodwright-rs (read only:
                              nothing is written into the Instance; the plan is checked against the vendored
                              contract, a tool failure exits 4); generate exits 2 with the reason until
                              lodwright-rs can generate
  nvmod nvse-usage --instance <dir> [--profile <name>] [--jip-table <file>] [--nvse-names <file>] [--json]
                              count the extender opcodes the compiled scripts of the active plugins call (read
                              only; prints only opcode numbers, command names from the tables you name, counts
                              and plugin file names; docs/nvse-usage.md)
  nvmod tools list|verify [--lock <file>] [--store <dir>] [--instance <dir>] [--json]
  nvmod tools contracts [--json]
                              the tool contracts this nvmod reads and the schema versions it accepts (the current
                              one and the one before it)
  nvmod tools fetch <tool> [--lock <file>] [--store <dir>] [--instance <dir>] [--gh <path>] [--curl <path>]
  nvmod tools trust <tool> <version> --instance <dir> [--sha256 <hex>]
  nvmod tools run <tool> [--lock <file>] [--store <dir>] [--instance <dir>] -- <args...>
                              the pinned tools of tools.lock.toml: list their state, re-check the stored files
                              without network, fetch one (only from the lock's URL; its SHA-256 is checked before
                              it is made executable, then its attestation), confirm a third-party tool that pins
                              no checksum, or run one (its SHA-256 is re-checked at every call)
  nvmod --version | --help

every command writes a run log to logs/<run id>.ndjson in the state directory ($NVMOD_STATE_DIR, else
$XDG_STATE_HOME/nvmod or ~/.local/state/nvmod; %LOCALAPPDATA%\\nvmod on Windows), with paths and secrets redacted; the
last 20 runs or 50 MB are kept and nothing is sent anywhere. --log-level <off|error|warn|info|debug|trace> (or
NVMOD_LOG) sets the text on standard error (default warn) and, above info, the log file.

exit codes:
  0  success
  1  problems found, or a FOMOD that needs answers
  2  refused (nothing was changed)
  3  verification failure
  4  step failed and rolled back
  5  command line not understood\n
SIGTERM or SIGINT during setup, install, ttw, import-mo2, mods or sort --apply rolls the step in flight back
(setup keeps a partial Instance with --keep-partial) and exits 4 with a `finished` record.\n";

/// Whether `args` is a command that writes to an Instance and so rolls back when it is stopped: `setup`,
/// `install`, `ttw`, `import-mo2`, `mods enable|disable|reorder`, `plugins enable|disable|move|lock|unlock` and `sort --apply`.
fn writes_instance(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        Some("setup" | "install" | "ttw" | "import-mo2") => true,
        Some("mods") => matches!(
            args.get(1).map(String::as_str),
            Some(
                "enable"
                    | "disable"
                    | "reorder"
                    | "remove"
                    | "rename"
                    | "create"
                    | "clone"
                    | "hide"
                    | "unhide"
                    | "note"
                    | "separator"
                    | "category"
            )
        ),
        Some("plugins") => matches!(
            args.get(1).map(String::as_str),
            Some("enable" | "disable" | "move" | "lock" | "unlock")
        ),
        Some("sort") => args.iter().any(|a| a == "--apply" || a == "--keep-order"),
        Some("backup") => args.get(1).map(String::as_str) == Some("restore"),
        Some("saves") => matches!(
            args.get(1).map(String::as_str),
            Some("backup" | "restore" | "transfer")
        ),
        Some("profile") => matches!(args.get(1).map(String::as_str), Some("apply" | "finish")),
        _ => false,
    }
}

/// [`run`] for the binary. For a writing command (see [`writes_instance`]) SIGTERM and SIGINT (Ctrl+C on
/// Windows) do not kill the process: they ask the step in flight to stop, which the journal machinery rolls
/// back (setup keeps a partial Instance with `--keep-partial`). The run then ends with exit code 4 and a
/// `finished` record: setup's own event, or for the other commands `{"event":"finished",...,"interrupted":true,
/// "outcome":"rolled-back"}` on standard output with `--json` and a line on standard error without.
pub fn run_main(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    if writes_instance(args) {
        // Best effort: without a handler the default action (end the process) stays.
        let _ = ctrlc::set_handler(nvmod_core::cancel::request);
    }
    let code = run(args, out, err);
    if code == ROLLED_BACK
        && nvmod_core::cancel::requested()
        && args.first().map(String::as_str) != Some("setup")
    {
        let command = args.first().map_or("nvmod", String::as_str);
        if args.iter().any(|a| a == "--json") {
            let _ = writeln!(
                out,
                "{}",
                serde_json::json!({
                    "event": "finished", "status": "failed", "exit_code": ROLLED_BACK,
                    "rolled_back": true, "resumable": false, "interrupted": true,
                    "outcome": "rolled-back", "command": command,
                })
            );
        } else {
            let _ = writeln!(
                err,
                "nvmod {command}: interrupted; the step in flight was rolled back"
            );
        }
    }
    code
}

/// Run the command line `args` (without the program name), writing normal output to `out` and errors to `err`.
/// Returns the process exit code.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    backup::around(args, err, |err| dispatch(args, out, err))
}

fn dispatch(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    match args.first().map(String::as_str) {
        None | Some("--help" | "-h") => {
            let _ = out.write_all(HELP.as_bytes());
            OK
        }
        Some("pe") => pe::run(&args[1..], out, err),
        Some("catalog") => catalog::run(&args[1..], out, err),
        Some("doctor") => doctor::run(&args[1..], out, err),
        Some("conflicts") => conflicts::run_conflicts(&args[1..], out, err),
        Some("plugins") => plugins::run(&args[1..], out, err),
        Some("tree") => conflicts::run_tree(&args[1..], out, err),
        Some("stage") => stage::run(&args[1..], out, err),
        Some("init") => init::run(&args[1..], out, err),
        Some("import-mo2") => import::run(&args[1..], out, err),
        Some("install") => install_mod::run_install(&args[1..], out, err),
        Some("rollback") => install_mod::run_rollback(&args[1..], out, err),
        Some("mods") => mods::run(&args[1..], out, err),
        Some("profile") => profile::run(&args[1..], out, err),
        Some("saves") => saves::run(&args[1..], out, err),
        Some("target") => target::run(&args[1..], out, err),
        Some("setup") => setup::run(&args[1..], out, err),
        Some("verify") => verify::run(&args[1..], out, err),
        Some("sort") => sort::run(&args[1..], out, err),
        Some("nvse-usage") => nvse_usage::run(&args[1..], out, err),
        Some("backup") => backup::run(&args[1..], out, err),
        Some("lod") => lod::run(&args[1..], out, err),
        Some("tools") => tools::run(&args[1..], out, err),
        Some("ttw") => ttw::run(&args[1..], out, err),
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
