# nvmod-rs

`nvmod` is a unified mod manager and installer for Fallout: New Vegas and Tale of Two Wastelands, written in Rust.
It is the installer of [nv-rs](https://github.com/JorisJonkers-dev/nv-rs): one tool that takes a profile from "a list
of mods" to "a runnable game with those mods, sorted and with LOD". The plan, with the survey of existing managers,
the virtual file system options and the milestones, is in
[`docs/plans/mod-manager.md`](https://github.com/JorisJonkers-dev/NV-Linux-Modding/blob/main/docs/plans/mod-manager.md)
of NV-Linux-Modding.

Licence: GPL-3.0-only (see [`LICENSE`](LICENSE)). The reason is in [`docs/DECISIONS.md`](docs/DECISIONS.md).

## Status: milestone M1, the read-only NVSE doctor

Nothing in this repository writes to a Mod Organizer 2 instance or to a game folder. The doctor reads:

- NVSE plugin DLLs, **without running them**: PE header, bitness, large-address-aware flag, exports (is it an NVSE
  plugin at all), the exported version struct, imported DLLs. The parser reads through bounded readers and is
  fuzzed, because the files are untrusted.
- A catalogue of NVSE plugins that this repository owns (`data/nvse-plugins.toml`, each entry with its provenance).
- A Mod Organizer 2 instance directory: `ModOrganizer.ini`, a profile's `modlist.txt` and `plugins.txt`, and each
  mod's `meta.ini`.

```text
nvmod pe <file.dll> [--json]                    inspect one PE file
nvmod catalog [--json]                          list the plugin catalogue
nvmod nvse-usage --instance <dir> [--json]      opcode usage of the active plugins' scripts (docs/nvse-usage.md)
nvmod doctor --instance <dir> [--profile <name>] [--game <dir>] [--json] [--fail-on <severity>]
```

Every command prints JSON with `--json`, so scripts and the headless harness never depend on a GUI.

## Layout

| Crate | Role |
| --- | --- |
| `nvmod-core` | Severity, findings, the case-insensitive file index |
| `nvmod-pe` | Safe PE inspector and NVSE plugin metadata (`unsafe_code = "forbid"`) |
| `nvmod-testkit` | Builds synthetic PE files in code; no real game or mod file is ever committed |
| `nvmod-catalog` | Loads and resolves `data/nvse-plugins.toml` |
| `nvmod-formats` | Readers for the MO2 instance (`modlist.txt`, `plugins.txt`, `meta.ini`, `ModOrganizer.ini`) |
| `nvmod-doctor` | The checks, composed into one report |
| `nvmod-cli` | The `nvmod` binary |

## Development

`mise install`, then `task check` runs what the pipeline runs. On NixOS without a linker:
`nix shell nixpkgs#gcc nixpkgs#lld -c cargo test` with `RUSTFLAGS="-Clink-self-contained=-linker"`. Heavy commands
go through `capped` (see [`docs/PRACTICES.md`](docs/PRACTICES.md)). Decisions and the questions the owner should
revisit are in [`docs/DECISIONS.md`](docs/DECISIONS.md).
