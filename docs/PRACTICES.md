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
| Releases | `release-please-config.json`, `.github/workflows/release.yml` | `release-type: simple` with `extra-files` for the version in `Cargo.toml` and `Cargo.lock`; the first release is pinned to `0.1.0` with `release-as` (remove the key after it ships). Nothing is published |
| Dependency updates | `renovate.json` | Extends the org preset; crates need to be at least 7 days old (`global-min-publish-age`) |
| Commits | Conventional Commits | The PR title is checked by the pipeline; squash merge uses it as the commit message |

## Provenance of data

Anything the repository states as a fact about the outside world (a plugin's DLL name, its Nexus id, a file
format field) carries where it came from: in `data/nvse-plugins.toml` each entry has a `[provenance]` table with
`source`, `verified_on` and `verified`. An entry that nobody checked says `verified = false`; the doctor then
reports what it infers from it as less certain. Format facts in code carry a comment naming their source, and
anything from memory says so (see `docs/DECISIONS.md`, "Unverified assumptions").

## Heavy local commands

Run builds, tests and fuzzing through `capped`, which limits memory (a worker once used 14.4 GB and froze the
machine): `capped 6G cargo test --workspace`. Without a linker, as on NixOS:
`RUSTFLAGS="-Clink-self-contained=-linker" capped 6G nix shell nixpkgs#gcc nixpkgs#lld -c cargo test`.
