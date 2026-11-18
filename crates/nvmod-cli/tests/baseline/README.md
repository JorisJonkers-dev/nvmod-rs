# Baseline of the commands nvmod removes

These transcripts record what `nvmod` 0.8.1 (the release tagged `v0.8.1`) printed and returned for the commands that
rauls-trusty-toolbox ADR-0019 moves out of nvmod: `tools contracts` (the issue calls it `contracts`), `verify`,
`tools`, `run`, `install-nv-rs`, `lod`, `ttw` and `setup`. They are the reference that raul's replacements are compared
with before the nvmod commands are removed (issue #90). A command that is already removed keeps its transcripts under
`removed/` (see [Removed commands](#removed-commands)).

Replay: `cargo test -p nvmod-cli --test baseline`. Re-record: see [Re-recording](#re-recording).

## Removed commands

`lod` was removed from nvmod in issue #166. Its 13 transcripts (`plan-*`, `generate-is-refused-with-reason`, `usage-*`)
moved unchanged to `removed/lod/`: they were recorded before removal, at release v0.8.1, and are the reference for
raul's `lod` replacement. The replay test in this crate does not run them (nvmod no longer has the command, so there is
nothing to replay) and ignores the `removed/` folder when it looks for stray files; the test
`removed_lod_transcripts_still_parse_and_are_unchanged` checks that they stay whole (valid JSON, the folder name as
`command`, the fields of the format, `release` `0.8.1`, 13 of them, and the SHA-256 of each as listed in
`removed/lod/SHA256SUMS`), and `lod_is_an_unknown_command` checks that nvmod exits 5 for it. Their fixtures (the fake lodwright, the pinned lock and the
golden plan `tests/golden/lodwright-plan.json`) and the setup functions of `tests/baseline.rs` that built them (`lod_with`, `pin_lodwright`, `lod_plan`, ...) were deleted
with the command; they are in the repository history at commit `599c22d` (the baseline, #182), the last one before
the removal. The tag `v0.8.1` has the command but not the baseline harness. Do not re-record them. The same applies to each command removed later.

## Layout

- `<command>/<case>.json` is one transcript. Its fields are `command` (the folder), `args` (as typed, with `<TMP>` for the
  scratch directory), `exit_code`, `stdout`, `stderr`, and `release` (the nvmod release the transcript was taken from; a
  note, not compared).
- `stdout` that is one JSON document is stored parsed. A `--json` stream (one document per line, as `setup` prints its
  events) is stored as an array of those documents. Other text is stored as an array of lines, so a changed line shows
  as a changed line in the diff.
- `stderr` is stored as an array of lines, after the toolchain filter below.
- The cases are defined in `tests/baseline.rs`: each case is a name, the arguments, and the name of a Rust setup function
  that builds its fixture under the scratch root. Nothing is recorded from a machine's own files.
- Fixtures are synthetic: a fake Steam base copy, a fake Fallout 3 folder, fake executables and plugins (`nvmod-testkit`),
  fake release servers on the loopback address, shell scripts for Proton, the TTW installer, `gh` and
  `cargo`. No game data, no mod names, no private paths.

## Normalisation

Every run is normalised by the same rules, recorded or replayed. A rule that is not listed here is not applied.

1. The scratch directory of the case, as given and canonicalised (`/private/var/...` on macOS), is written `<TMP>`.
   The arguments use `<TMP>` too; the test expands it to the real directory before the run.
2. The fake release asset name (`nvmod_testkit::FAKE_ASSET`) is written `<ASSET>`. It contains the host platform
   string, which is not otherwise shown.
3. The host platform string (`nvmod_tools::platform()`, for example `linux-x86_64`) is written `<PLATFORM>`. It keys the
   lock files the fixtures write, and nvmod does not print it in these transcripts, so no transcript holds `<PLATFORM>`
   today; the rule is there so that a transcript that prints it is portable. The launch plan's `linux` and `windows`
   entries are nvmod's own names for the two targets, not the host, and are not normalised.
4. The release in nvmod's own lines, `nvmod 0.8.1`, is written `nvmod <VERSION>`, as a whole token: `nvmod 0.8.10`
   and the versions of tools, schemas and contracts (`lodwright-plan v1`) are never replaced. No transcript holds
   `<VERSION>` today; the rule is covered by the unit test `normalisation_is_whole_token_and_leaves_other_numbers_alone`.
5. A timestamp (`YYYY-MM-DD` then `T` or a space, `hh:mm:ss`, an optional fraction and a `Z` or `±hh:mm` zone) is
   written `<TIME>`. No transcript holds one today; covered by the same unit test.
6. A journal id, `<16 digits>-<6 digits>-<process id>`, is written `<JOURNAL>`: it is made from the clock and the
   process.
7. The port of a fake server, which the OS picks, after `127.0.0.1:` is written `<PORT>`. No transcript holds one
   today; covered by the unit test.
8. A SHA-256 value (64 lowercase hex digits, as a whole word) is written `<SHA256-n>`, where `n` numbers the distinct
   digests of the run in order of first appearance. Standard output and standard error of one run share the numbering,
   so equal digests keep equal numbers and a wrong digest or a swapped pair still shows as a difference. The fake release
   archives carry the file times of the run, so their digests change from one run to the next; the numbering keeps the
   relation between them.

Standard error is filtered before rule 8 (only standard error): the lines the toolchain prints when a source build runs
are dropped. Those are cargo's progress (`Compiling`, `Finished`, `Running`, ...), rustc's diagnostics (`error:`,
`warning:`, `-->`, `|`, `= note`, the snippet lines `12 | code`, `For more information`) and blank lines. What remains is
nvmod's own standard error (`nvmod ...:` lines and their continuation lines), which is what the transcript is about.
The exit code is always recorded.

Nothing else is normalised. The arguments are not normalised: a digest given on a command line is recorded as typed.

### The environment of a run

`nvmod` runs with `env_clear()` and only three variables set: `PATH`, `HOME` (a folder in the scratch root) and
`NVMOD_STATE_DIR` (a folder in the scratch root). The current directory is the scratch root.

`PATH` is the scratch `bin` folder and nothing else. The fixtures put only the programs a case needs into that folder,
as symbolic links to the test's own copies: `cat`, `cp` and `mkdir` for the fixture scripts; `curl` for the cases that
fetch or trust a tool; `tar` and `gzip` for the cases that name an nv-rs release. Nothing else is linked, so no host
program that nvmod looks for by name is visible to a case. In particular the archive tools that nvmod tries for a mod
archive (`7zz`, `7z`, `7za`, `bsdtar`) are never on the PATH, and the refusal of a non-archive is the same on every Unix.
A test run that has no `cat`, `cp`, `mkdir`, `curl`, `tar` or `gzip` on its own PATH fails with the name of the missing
program.

The one case that runs another program of the build is a source build of nv-rs (`install-nv-rs --source`). Its `--cargo` is a wrapper script in the scratch `bin` folder, which sets its own
environment and nothing else: `PATH` (the toolchain folder that built the tests, the folder of `cc` found on the test's
`PATH` since a linker is needed, `/usr/bin`, `/bin`; the wrapper is the only process that sees these), `CARGO_HOME` and `CARGO_TARGET_DIR` under the scratch root,
`RUSTC` (the toolchain's `rustc`), and `CARGO_NET_OFFLINE=true`. The scratch root is removed when the case ends, so the
build output does not outlive it. The wrapper's values are fixtures: they are not in any transcript.

### Every invocation is bounded

Each `nvmod` run has 120 seconds. It runs in its own process group. A run that exceeds the limit, or a test that panics
while a run is alive, is killed with its whole group (the program and what it started), and the test fails with the
output captured so far.

## Platforms

The fixtures run on the host that runs the tests (macOS and Linux CI both run them; the module is `#![cfg(unix)]`, so
Windows compiles the crate doc only). Host-dependent values are normalised (rules 2 and 3), and the lock files the
fixtures write use the host platform, so a transcript recorded on one OS replays on another. The shell scripts are
`#!/bin/sh` and use only the builtins `echo`, `exit`, `exec`, `for` and `[`, and the linked `cat`, `cp` and `mkdir`;
no script uses an escape sequence in `echo`. No transcript prints the platform, the host PATH, or a host file name.

Checked for host-tool dependencies (macOS against Linux): `sed`, `file`, `sha256sum`/`shasum`, `mktemp` and `ls` are not
used by the harness or its scripts; the digests are computed in-process by nvmod and normalised; the scratch root is
normalised both as given and canonicalised (`/private/var` on macOS); the release archive is written by `tar -czf ... -C`,
which GNU tar and bsdtar both accept, and its bytes (and so its digest) are normalised; fixture file names are
distinct when case is ignored, so a case-insensitive file system does not merge them; I found no transcript that
lists directory entries, so directory order (which differs between file systems) does not reach a transcript as far as
the cases go; a macOS diff of that kind would show up as a changed line and is a bug to fix in the output. The process group is killed with
`kill -s KILL -- -<pid>`, which BSD and GNU kill both accept.

## Re-recording

`NVMOD_BLESS=1 cargo test -p nvmod-cli --test baseline` rewrites every transcript from the current build.

- `NVMOD_BLESS` must be `1` or `true`. Any other value, the empty string included, fails the test.
- It is refused when `CI` is set: a transcript is recorded on a workstation and reviewed in the diff, never written by a
  pipeline.
- Review the diff before committing it. A changed transcript is a changed behaviour that the removal must account for.
  A new nvmod release changes only the `release` field and, where printed, `<VERSION>`, so a bless there shows no other
  diff.

## Reusing this in the Toolbox

The transcript format is the contract. A raul-side replay needs:

- the same `nvmod`-independent harness: for each transcript, run the binary named by raul's own build of the Component
  with `args` expanded (`<TMP>` to a fresh scratch directory), the environment as in [The environment of a run](#the-environment-of-a-run),
  and a 120-second bound;
- the same normaliser (rules 1 to 8 and the stderr filter), so that its output is compared with the transcript as the
  rules define it; the rules are in `tests/baseline.rs` (`Norm`, `drop_toolchain_output`) and must be ported, not
  re-derived;
- fixtures from the setup functions in `tests/baseline.rs`, keyed by the case name. The setup functions build the
  synthetic fixtures with `nvmod-testkit` and call `nvmod` for the steps that make an Instance; a raul replay that
  wants the same fixtures calls these functions (or ports them) rather than writing its own.

Comparison is on the parsed values: `stdout` and `stderr` as stored, `exit_code` as an integer. `release` is ignored.

## Coverage

Cases per command (the file names). `usage-*` cases exit 5 unless the table says otherwise.

| Command | Cases | Notes |
| --- | --- | --- |
| `tools contracts` | `tools-contracts-json`, `tools-contracts-text`, `tools-contracts-unknown-option`, `top-level-contracts-is-unknown` | There is no top-level `contracts` command in 0.8.1: `contracts` is `nvmod tools contracts`, and the bare word is a usage error (exit 5). |
| `tools list` | `list-json`, `list-text`, `list-future-lock-is-refused` (exit 2), `list-unknown-option` (exit 5) | A fixture lock pins `lodwright-rs` to bytes a fake server serves; `loot-rs` is linked and `ttw-installer` has no checksum. The shipped `tools.lock.toml` is not read, because its pins change per release. |
| `tools verify` | `verify-not-fetched-json`, `verify-after-fetch-json`, `verify-after-fetch-text`, `verify-future-lock-is-refused` (exit 2), `verify-unknown-option` (exit 5) | |
| `tools fetch` | `fetch-text`, `fetch-checksum-mismatch` (exit 3), `fetch-unknown-tool` (exit 2), `fetch-missing-tool-is-usage-error` (exit 5), `fetch-unknown-option` (exit 5) | The `gh` of the attestation is a script that accepts. |
| `tools run` | `run-fetched`, `run-not-fetched-is-refused` (exit 2), `run-missing-tool-is-usage-error` (exit 5), `run-unknown-option` (exit 5) | The fetched tool prints a fixed JSON document. |
| `tools trust` | `trust-pinned-tool-is-refused` (exit 2), `trust-third-party-tool` (exit 2, no digest given), `trust-third-party-with-sha256` (exit 0), `trust-missing-arguments-is-usage-error` (exit 5), `trust-unknown-option` (exit 5) | `trust-third-party-with-sha256` passes the digest of the served bytes as typed. |
| `tools` usage | `usage-no-subcommand`, `usage-unknown-subcommand` | Exit 5. |
| `verify` | `clean-json`, `clean-text`, `clean-named-proton`, `doctor-error-json` (exit 3), `doctor-error-text` (exit 3), `nv-rs-changed-json` (exit 3), `unknown-profile` (exit 2), `not-an-instance` (exit 2), `missing-instance-option` (exit 5), `unknown-option` (exit 5) | |
| `run` | `dry-run-game-json`, `dry-run-game-text`, `dry-run-game-with-game-args`, `launch-game-wait`, `launch-game-wait-exit-code-passes-through` (exit 7), `unknown-title-is-refused` (exit 2), `nv-rs-not-installed-is-refused` (exit 2), `not-an-instance-is-refused` (exit 2), `missing-instance-option` (exit 5), `unknown-option` (exit 5) | The "Proton" is a script that prints its arguments and exits 0, or 7 for the pass-through case. The game is never started. Only `--wait` launches are recorded: a detached start has no exit code to record. |
| `install-nv-rs` | `source-build-installs-and-checks`, `source-build-failing-build-is-refused-and-rolled-back` (exit 4), `source-build-failing-check-is-refused-and-rolled-back` (exit 4), `release-fetched-verified-and-installed`, `release-wrong-checksum-installs-nothing` (exit 3), `release-failing-check-rolled-back` (exit 4), `release-asset-not-listed` (exit 3), `refused-without-source-or-release` (exit 2), `refused-not-an-instance` (exit 2), `usage-*` (exit 5) | The command has no `--json`, so its transcripts are text. The source build uses the cargo wrapper above; its failing-build transcript keeps nvmod's line only, so it does not depend on the pinned toolchain's diagnostics. The release is a fake one served from `file://`. |
| `ttw` | `installs-text`, `installs-json`, `installer-fails-exit-4-rolled-back`, `report-says-failed-exit-4-rolled-back`, `report-breaks-contract-exit-4-rolled-back`, `installer-writes-nothing-exit-4-rolled-back`, `installer-not-the-pinned-file-exit-3`, `installer-unpinned-is-refused` (exit 2), `lock-without-installer-is-refused` (exit 2), `bad-base-is-refused-before-installer` (exit 2), `missing-fallout3-folder-is-refused` (exit 2), `usage-missing-fallout3-option` (exit 5), `usage-unknown-option` (exit 5) | The installer is a shell script that writes a synthetic output folder and a report in the `ttw-installer-result` shape. The shipped lock pins no installer, so every case passes its own lock. |
| `setup` | `full-run-json`, `full-run-text`, `dry-run-text`, `dry-run-json`, `skip-nv-rs-json`, `skip-nv-rs-without-ttw-options-json`, `failing-nv-rs-check-exit-4-leaves-no-instance`, `wrong-release-checksum-exit-3-leaves-no-instance` (exit 3), `failing-mod-install-rolls-back` (exit 2), `failing-mod-install-keep-partial` (exit 2), `existing-instance-refused` (exit 2), `unsupported-base-refused` (exit 2), `resume-without-state-refused` (exit 2), `restart-without-yes-refused` (exit 2), `usage-missing-required-option` (exit 5), `usage-unknown-option` (exit 5) | The full run takes the fake nv-rs release from `file://`, the fake TTW installer (pinned by its own lock), one synthetic mod archive, the synthetic masterlist and `sort --apply`. `failing-mod-install-*` use an archive that is not a zip: that step exits 2 (refused), not 4, and the transcript records it. |

## Not recorded

- `install-nv-rs` with `NVMOD_CARGO` set in the environment: the environment of a case is fixed by the test, so the
  toolchain comes from `--cargo` only.
- `setup --resume` after a partial run, and `setup --restart --yes`: they need an Instance left by an earlier failed run.
  A fixture that builds one would have to be recorded in sequence; the single-case transcripts do not do that.
- Detached `run` (no `--wait`): nothing to record beyond the plan, and the plan is the `--dry-run` transcript.
- `ttw` and `setup` with a real TTW package or a real base copy: no game data is used anywhere in this baseline.
- `tools fetch` over a real network, and nv-rs releases from GitHub: every release is a local fake.
