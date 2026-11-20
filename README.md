# nvmod-rs

`nvmod` is the mod-management Component of the stack, for Fallout: New Vegas and Tale of Two Wastelands, written in
Rust. The product that players install is
[Raul's Trusty Toolbox](https://github.com/JorisJonkers-dev/rauls-trusty-toolbox) (`raul`); it links nvmod's libraries
and shows its reports. `nvmod` also works alone from the command line, and it never fetches, finds or starts another
Component.

**Systems.** Built and tested on Linux, Windows and macOS on every pull request (the known Windows and macOS test
failures are #98). Each release attaches the command for Linux x86-64, Windows x86-64 and macOS (one binary for Apple
silicon and Intel); see [docs/releasing.md](docs/releasing.md).

## Scope

nvmod-rs keeps mod management:

- the Instance, Profile and Mod model and its schema;
- mod install, FOMOD, the mod lifecycle, `meta.ini`, separators and categories;
- the Staging tree, file and archive conflicts, and Overwrite;
- Plugins and the Load order (it links [loot-rs](https://github.com/JorisJonkers-dev/loot-rs)), including the
  minimal-change sort;
- backups, saves, MO2 import, the Nexus client, the downloads library and mod update rows;
- the executables registry, as data;
- the content Doctor: the NVSE plugin catalogue, readiness per category from a given `capabilities.json`, and the
  opcode usage scan.

These move to the Toolbox, as decided in rauls-trusty-toolbox ADR-0007 and planned in its `docs/REORG.md`:

- setup (removed in #164; raul composes it from the mod-management jobs), `ttw` (removed in #165; raul runs TTW through ttwright), `lod` (removed in #166; raul runs LOD), the engine install (`install-nv-rs`, removed in #168), `run`,
  `tools`, `verify` and `contracts`;
- the subprocess Runner (`nvmod-runner`) and tool supply (`nvmod-tools`, `tools.lock.toml`).

The GUI is raul's alone (rauls-trusty-toolbox ADR-0019); nvmod has no GUI, view model or service seam.

Each moved command is removed from nvmod when it moves; use `raul` for it from then on. Until then the commands work
as documented.

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
revisit are one file each in [`docs/decisions/`](docs/decisions/README.md); earlier ones are in [`docs/DECISIONS.md`](docs/DECISIONS.md).
