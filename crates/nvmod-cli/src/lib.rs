// SPDX-License-Identifier: GPL-3.0-only
//! The `nvmod` command line. Every command prints JSON with `--json`, so scripts and the harness never depend
//! on a GUI.

mod args;
mod backup;
mod catalog;
mod conflicts;
mod doctor;
mod downloads;
mod exec;
mod import;
mod init;
mod install_mod;
mod install_nvrs;
mod lock;
mod logging;
mod mods;
mod mods_life;
mod nvse_usage;
mod pe;
mod plugins;
mod profile;
mod profile_apply;
mod profile_ops;
mod run;
mod saves;
mod sort;
mod stage;
mod target;
mod tools;
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
                [--answers <file>] [--max-extract-bytes <n>] [--max-extract-entries <n>]
                [--max-extract-entry-bytes <n>] [--max-extract-ratio <n>] [--max-extract-depth <n>]
                [--max-extract-operations <n>] [--max-extract-seconds <n>] [--json]
                              install a Mod from a zip, 7z, tar or compressed tar archive (rar is refused); a FOMOD is
                              answered where its XML is unambiguous and otherwise exits 1 with the questions
                              (--json), which --answers completes; then stage the profile. The --max-extract-*
                              flags raise or lower the extraction limits (defaults in docs/DECISIONS.md); an archive
                              past one is refused (exit 2) and the Instance is left as it was
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
  nvmod downloads list --instance <dir> [--downloads <dir>] [--unmatched] [--hash] [--json]
  nvmod downloads show <archive> --instance <dir> [--downloads <dir>] [--hash] [--json]
  nvmod downloads install <archive> --instance <dir> [--downloads <dir>] [--profile <name>] [--name <mod name>]
                [--dry-run] [--answers <file>] [--max-extract-bytes <n>] [--json]
  nvmod downloads hide|unhide|forget <archive> --instance <dir> [--downloads <dir>] [--dry-run] [--json]
                              the downloads library (default <instance>/downloads): each archive with the state
                              MO2 keeps in its .meta file (installed, uninstalled, hidden, paused), its Nexus mod
                              and file ids (from the .meta or the Nexus file name), the Mod it matches and, with
                              --hash, SHA-256 and MD5 (cached by size and time); --unmatched lists the archives
                              with no Mod and the Mods with no archive; install is `nvmod install` on the archive
                              plus the .meta marked installed; hide and unhide edit only the .meta; forget moves
                              the archive and its .meta to .nvmod/trash/downloads (nothing is deleted); no network
  nvmod conflicts --instance <dir> [--profile <name>] [--mod <name>] [--archives] [--json]
                              the file-level conflicts of a profile (read only): who wins each contested path,
                              and per Mod whether it overwrites, is overwritten, is mixed or redundant;
                              --archives adds the BSA axes: the archives the game loads and where a loose file
                              or another archive collides with an archive (rules marked unverified)
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
  nvmod profile create --instance <dir> [--name <name>]
                              make an empty profile (default name `Default`)
  nvmod profile list --instance <dir> [--json]
  nvmod profile copy <from> <to> --instance <dir> [--with-saves] [--dry-run] [--json]
  nvmod profile rename <from> <to> --instance <dir> [--dry-run] [--json]
  nvmod profile delete <name> --instance <dir> [--dry-run] [--json]
  nvmod profile select <name> --instance <dir> [--json]
  nvmod profile set <LocalSaves|LocalSettings|AutomaticArchiveInvalidation> <true|false> --instance <dir>
                [--profile <name>] [--dry-run] [--json]
                              manage the profiles: list them with their enabled counts and switches, copy the lists
                              and INIs (--with-saves: saves/ too; never the Mods), rename or delete one (refused for
                              the selected profile, or while the game runs; delete moves the folder to
                              .nvmod/trash/; both are undone by `nvmod rollback`), select the active profile and
                              record a per-profile switch in its settings.ini (a launch applies it)
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
  nvmod verify --instance <dir> [--profile <name>] [--masterlist <file>] [--catalog <file>] [--proton <program>]
                [--fail-on <severity>] [--json]
                              confirm an install (read only): the Doctor on the Instance, the installed nv-rs
                              against its install record, and the launch plan of the vanilla plus xNVSE path
                              (a Linux entry that invokes Proton, a Windows entry that runs the game executable
                              directly); exit 3 when verification fails
  nvmod nvse-usage --instance <dir> [--profile <name>] [--jip-table <file>] [--nvse-names <file>] [--plugin-names <file>] [--json]
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
  nvmod exec list|add|edit|remove|import --instance <dir> ...
                              the executables registry (title, binary, arguments, working directory, Steam app
                              id, hidden); `import --mo2 <dir>` reads MO2's customExecutables (only Z: paths map)
  nvmod run <title|path|game|nv-rs> --instance <dir> [--profile <name>] [--proton <program>] [--dry-run]
                [--wait] [--restage] [--no-stage] [--no-profile] [--json] [-- <args>...]
                              start the game or a tool through the staged view: refuses while Mod Organizer 2
                              runs or another command holds the Instance lock (exit 2), stages the profile when
                              the tree is missing or old, then starts the program (detached unless --wait, whose
                              exit code passes through); --dry-run prints the plan and changes nothing
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
SIGTERM or SIGINT during install, import-mo2, mods or sort --apply rolls the step in flight back
and exits 4 with a `finished` record.\n";

/// Whether `args` is a command that writes to an Instance and so rolls back when it is stopped: `install`, `import-mo2`, `mods enable|disable|reorder`, `plugins enable|disable|move|lock|unlock` and `sort --apply`.
fn writes_instance(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        Some("install" | "import-mo2") => true,
        Some("downloads") => matches!(args.get(1).map(String::as_str), Some("install")),
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
        Some("profile") => matches!(
            args.get(1).map(String::as_str),
            Some("apply" | "finish" | "copy" | "rename" | "delete" | "select" | "set")
        ),
        _ => false,
    }
}

static PROCESS_TOKEN: std::sync::OnceLock<nvmod_api::CancelToken> = std::sync::OnceLock::new();

/// The token every job of this process runs with: what SIGINT and SIGTERM (Ctrl+C on Windows) cancel. A job that is
/// given a token obeys that token only (nvmod-rs#212), so the command line hands its jobs this one.
pub(crate) fn process_token() -> nvmod_api::CancelToken {
    PROCESS_TOKEN
        .get_or_init(nvmod_api::CancelToken::new)
        .clone()
}

/// [`run`] for the binary. For a writing command (see [`writes_instance`]) SIGTERM and SIGINT (Ctrl+C on
/// Windows) do not kill the process: they ask the step in flight to stop, which the journal machinery rolls
/// back. The run then ends with exit code 4 and a `finished` record: `{"event":"finished",...,"interrupted":true,
/// "outcome":"rolled-back"}` on standard output with `--json` and a line on standard error without.
pub fn run_main(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    if writes_instance(args) {
        // Best effort: without a handler the default action (end the process) stays.
        let _ = ctrlc::set_handler(|| {
            nvmod_core::cancel::request();
            process_token().cancel();
        });
    }
    let code = run(args, out, err);
    if code == ROLLED_BACK && nvmod_core::cancel::requested() {
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

/// Print a failed job of the library API (`nvmod-api`) the way the command line always has, and return its exit
/// code: a refusal is 2, a verification failure 3, a failed step 4.
pub(crate) fn job_failure(command: &str, e: &nvmod_api::JobError, err: &mut dyn Write) -> u8 {
    match e.kind {
        nvmod_api::ErrorKind::Refused => {
            let _ = writeln!(err, "nvmod {command}: refused: {}", e.summary);
            REFUSED
        }
        nvmod_api::ErrorKind::Corrupt => {
            let _ = writeln!(
                err,
                "nvmod {command}: verification failed: {}; nothing was changed",
                e.summary
            );
            VERIFICATION
        }
        _ => {
            let _ = writeln!(err, "nvmod {command}: {}", e.summary);
            ROLLED_BACK
        }
    }
}

/// `nvmod run`, with the program started by `launcher` instead of by `nvmod-runner`. `args` are the words after
/// `run` (the target, `--instance`, and so on), exactly as the command takes them. Planning, the Instance lock,
/// the Staging tree and the log header are the command's own; only the start of the program is the caller's, so a
/// program that links nvmod can start the game as a child of its own binary and still get everything else.
pub fn run_with_launcher(
    args: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
    launcher: &dyn nvmod_run::Launcher,
) -> u8 {
    run::run_with(args, out, err, launcher)
}

/// Run the command line `args` (without the program name), writing normal output to `out` and errors to `err`.
/// Returns the process exit code.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    lock::around(args, err, |err| {
        backup::around(args, err, |err| dispatch(args, out, err))
    })
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
        Some("downloads") => downloads::run(&args[1..], out, err),
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
        Some("verify") => verify::run(&args[1..], out, err),
        Some("exec") => exec::run(&args[1..], out, err),
        Some("run") => run::run(&args[1..], out, err),
        Some("sort") => sort::run(&args[1..], out, err),
        Some("nvse-usage") => nvse_usage::run(&args[1..], out, err),
        Some("backup") => backup::run(&args[1..], out, err),
        Some("tools") => tools::run(&args[1..], out, err),
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
