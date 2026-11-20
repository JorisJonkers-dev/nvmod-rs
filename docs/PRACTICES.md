# What the repository adds

Same conventions as `loot-rs` and `lodwright-rs`, adapted. Differences from `lodwright-rs`: GPL-3.0-only, the
repository is public (so CodeQL is on), merges are squash only.

| Practice | Where | Notes |
| --- | --- | --- |
| One required check, `Pipeline Complete` | `.github/workflows/pipeline.yml`, `.github/rulesets/main.json` | One job; the checks are steps (`task check`). Documentation-only changes skip the Rust steps. The org's reusable hygiene, PR-title and quarantine workflows are jobs that the last step gates on. Linux only: CI minutes are limited |
| Cancel superseded runs | `concurrency` in `pipeline.yml` | A new commit cancels the older run of a pull request; `main` is never cancelled |
| Actions pinned by SHA | every workflow | The org's reusable workflows at github-workflows v0.50.3; Renovate proposes bumps. Allowed actions are listed in `scripts/setup-repo.sh` |
| Branch protection | `.github/rulesets/`, `scripts/apply-ruleset.sh`, `scripts/setup-repo.sh` | Pull requests only, squash only, linear history, no force push, `Pipeline Complete` required; `v*` tags immutable; CodeQL default setup for `actions` and `python` (Rust is not offered by default setup) |
| Format, clippy, tests, coverage | `task fmt`, `clippy`, `test` | clippy pedantic with warnings denied; 80% line coverage gate (cargo-llvm-cov) |
| Tests | unit, property (`proptest`), golden, CLI | Golden files are JSON reports of synthetic files; `UPDATE_GOLDEN=1 cargo test` rewrites them and the diff is reviewed |
| Fuzzing | `fuzz/`, `.github/workflows/fuzz.yml` | Own workspace; one target per parser of untrusted input, type-checked on every pull request, run weekly |
| Licences, bans, sources, advisories | `deny.toml`, `task deny`, `.github/workflows/security.yml` | GPL-3.0-only project: permissive dependencies only, nothing allowed by default; RustSec daily |
| Secret scanning | `.gitleaks.toml`, `task secrets` | Every commit |
| Workflow lint | `actionlint`, `zizmor` in `task lint:actions` | |
| Repository guards | `scripts/repo_guards.py`, `task guards` | SPDX headers; no game data, mod archives or Windows binaries (no allow-list); `.gitignore` lists those extensions |
| Releases | `release-please-config.json`, `.github/workflows/release.yml` | `release-type: simple` with `extra-files` for the workspace version in `Cargo.toml` (every crate inherits it; `scripts/repo_guards.py release` checks that), and release.yml refreshes the lock files; the first release is pinned to `0.1.0` with `release-as` (remove the key after it ships). Nothing is published |
| Run log | `crates/nvmod-runlog`, `docs/PRACTICES.md` (below) | ADR-0012 of nv-linux: `tracing` in the library crates, one NDJSON log per command in the state directory, redacted at the writer, no network sink |
| Dependency updates | `renovate.json` | Extends the org preset; crates need to be at least 7 days old (`global-min-publish-age`) |
| Commits | Conventional Commits | The PR title is checked by the pipeline; squash merge uses it as the commit message |

## Provenance of data

Anything the repository states as a fact about the outside world (a plugin's DLL name, its Nexus id, a file
format field) carries where it came from: in `data/nvse-plugins.toml` each entry has a `[provenance]` table with
`source`, `verified_on` and `verified`. An entry that nobody checked says `verified = false`; the doctor then
reports what it infers from it as less certain. Format facts in code carry a comment naming their source, and
anything from memory says so (see `docs/DECISIONS.md`, "Unverified assumptions"; new decisions are files in `docs/decisions/`).

## Heavy local commands

Run builds, tests and fuzzing through `capped`, which limits memory (a worker once used 14.4 GB and froze the
machine): `capped 6G cargo test --workspace`. Without a linker, as on NixOS:
`RUSTFLAGS="-Clink-self-contained=-linker" capped 6G nix shell nixpkgs#gcc nixpkgs#lld -c cargo test`.

## The run log

Every `nvmod` command except `--help` and `--version` writes `logs/<run_id>.ndjson` (ADR-0012 of nv-linux). The
library crates only emit `tracing` events (spans carry `run_id`, `op`, `step` and `tool`); the `nvmod` binary
installs the subscriber through `nvmod_cli::run_logged`, which any binary that links the CLI crate can call in the same way (and installs
`tracing-log`'s `LogTracer` first, so `log` records reach the same log). Nothing is sent anywhere.

- **Where:** `$NVMOD_STATE_DIR/logs`, else `$XDG_STATE_HOME/nvmod/logs` or `~/.local/state/nvmod/logs`, on Windows
  `%LOCALAPPDATA%\nvmod\logs`. The last 20 runs or 50 MB are kept. `cargo test` sets `NVMOD_STATE_DIR` to
  `target/test-state` (`.cargo/config.toml`), so tests never touch the real one.
- **Format:** the first line is a header (`kind`, `log_schema`, `run_id`, version, operating system, architecture);
  every other line has the keys `ts level run_id op step tool tool_version exit_code duration_ms msg`, with `null`
  for a key that does not apply. The log is **not a contract**: keys may change with `log_schema`. `--json` output
  on standard output is unchanged by it.
- **Verbosity:** standard error shows `warn` and up by default (nothing was added there); `--log-level
  <off|error|warn|info|debug|trace>` or `NVMOD_LOG` changes that. The file keeps `info` and up, or the chosen level
  when that is more verbose. `--log-level` may stand anywhere before a `--`.
- **Redaction** (`nvmod-runlog`): `Secret` prints `[redacted]`; request bodies and file contents are never logged.
  At the writer the home directory becomes `~`, folders given as `--instance`, `--base`, `--game`, `--fallout3`,
  `--mo2`, `--source` and `--store` become `$INSTANCE`, `$BASE` and so on, every other absolute path becomes
  `<path>`, the value after `token`, `password`, `secret`, `authorization` or `bearer` and key-like words are
  masked (hashes stay). That last part is a best effort, not a guarantee; `crates/nvmod-runlog/tests/canary.rs` plants
  a fake token and a path outside the home directory and fails if either reaches the file.
- **Tools:** the runner logs each call (exit code, duration) and its standard error (capped at 64 KiB, redacted
  like the rest), and gives the tool `NVMOD_RUN_ID` and `NVMOD_LOG_FORMAT=json` as the only extra environment.
- **A panic** prints that it is a bug, where the run log is and `nvmod support-bundle` (the command itself is a
  later step of ADR-0012); the exit code is unchanged.
