# nvmod-rs

`nvmod` manages mods for Fallout: New Vegas and Tale of Two Wastelands, in Rust, on Linux, Windows and macOS. It keeps
an Instance (the game's mods, profiles and downloads in a Mod Organizer 2 style layout), installs mods from their
archives (FOMOD installers included), stages each profile into a data folder the game reads, finds file conflicts,
keeps the plugins and their load order, backs up saves and profiles, and checks the whole set with its content
Doctor. Your game folder is never written to.

Players use it through [raul](https://github.com/JorisJonkers-dev/rauls-trusty-toolbox), which links nvmod's
libraries and is the only window. `nvmod` also works alone from the command line.

## Used from raul

raul's **Mods** page is nvmod: install a mod from its archive, answer a FOMOD installer's questions in the window,
turn mods on and off, change their priority, remove them, and undo any of it. Each change takes an automatic snapshot
first, as the command line does.

<p align="center">
  <img src="https://raw.githubusercontent.com/JorisJonkers-dev/rauls-trusty-toolbox/main/crates/raul-gui/tests/snapshots/mods/mods-listed.png" alt="raul's Mods page: the mods with their priority and the selected mod's actions" width="49%">
  <img src="https://raw.githubusercontent.com/JorisJonkers-dev/rauls-trusty-toolbox/main/crates/raul-gui/tests/snapshots/mods/mods-install-questions.png" alt="Installing a mod with an installer: its questions in raul's window" width="49%">
</p>

The **Load order** page lists the plugins and sorts them with nvmod's sort, which uses
[loot-rs](https://github.com/JorisJonkers-dev/loot-rs) and the LOOT masterlist. raul shows the moves and the
masterlist's messages before anything changes. The Home page's load order check is nvmod's Doctor.

<p align="center">
  <img src="https://raw.githubusercontent.com/JorisJonkers-dev/rauls-trusty-toolbox/main/crates/raul-gui/tests/snapshots/load-order/load-order-preview.png" alt="raul's Load order page: a preview of three moves with the masterlist's messages" width="80%">
</p>

raul's setup makes the Instance with nvmod's jobs, and every nvmod command is also available as `raul <command>`
(`raul nvmod-help` lists them).

## On its own

```sh
nvmod init --base "<game folder>" --instance ~/fnv                 # a new Instance from a game copy
nvmod import-mo2 --mo2 <MO2 folder> --instance ~/fnv --base "<game folder>"
nvmod install <mod archive> --instance ~/fnv --interactive         # a FOMOD installer asks on the terminal
nvmod mods list --instance ~/fnv
nvmod stage --instance ~/fnv                                       # the profile's data folder
nvmod sort --instance ~/fnv --fetch-masterlist                     # print the LOOT order; --apply writes it
nvmod conflicts --instance ~/fnv
nvmod doctor --instance ~/fnv
nvmod rollback --instance ~/fnv                                    # undo the last change
nvmod --help                                                       # every command
```

Other commands handle profiles, plugins, downloads, saves, backups and Overwrite. Every command prints JSON with
`--json`, so scripts never depend on a window. Programs link the `nvmod-api` crate: typed jobs with progress and
cancel, the way raul calls them.

Each [release](https://github.com/JorisJonkers-dev/nvmod-rs/releases/latest) attaches the command for Linux x86-64,
Windows x86-64 (portable) and macOS (one binary for Apple silicon and Intel), with `SHA256SUMS`, an SBOM and an
artifact attestation; see [docs/releasing.md](docs/releasing.md).

## Why a separate project

nvmod is one of the Components raul is made of, next to [nv-rs](https://github.com/JorisJonkers-dev/nv-rs),
[loot-rs](https://github.com/JorisJonkers-dev/loot-rs), [lodwright-rs](https://github.com/JorisJonkers-dev/lodwright-rs)
and [ttwright-rs](https://github.com/JorisJonkers-dev/ttwright-rs). Each does one job, is tested on its own and
releases on its own schedule; raul links them into one program and ships one tested set. nvmod keeps mod management
only: raul decides the order of the steps of a setup, runs Tale of Two Wastelands, LOD and the game, and owns the
window (Toolbox [ADR-0019](https://github.com/JorisJonkers-dev/rauls-trusty-toolbox/blob/main/docs/adr/0019-raul-orchestrates-nvmod-manages-mods.md)).

## What it covers

- the Instance, Profile and Mod model and its schema;
- mod install, FOMOD, the mod life cycle, `meta.ini`, separators and categories;
- the Staging tree, file and archive conflicts, and Overwrite;
- Plugins and the Load order, including the minimal-change sort;
- backups, saves, MO2 import, the downloads library and mod update rows;
- the executables registry, as data;
- the content Doctor: the NVSE plugin catalogue, readiness per category from a given `capabilities.json`, and the
  opcode usage scan ([docs/doctor-readiness.md](docs/doctor-readiness.md), [docs/nvse-usage.md](docs/nvse-usage.md)).

## Development

`mise install`, then `task check` runs what the pipeline runs. On NixOS without a linker:
`nix shell nixpkgs#gcc nixpkgs#lld -c cargo test` with `RUSTFLAGS="-Clink-self-contained=-linker"`. Heavy commands
go through `capped` (see [`docs/PRACTICES.md`](docs/PRACTICES.md)). Test files are built in code by `nvmod-testkit`;
no game or mod file is ever committed. Decisions are one file each in [`docs/decisions/`](docs/decisions/README.md);
earlier ones are in [`docs/DECISIONS.md`](docs/DECISIONS.md).

## Legal notice

nvmod contains no game files, no mod files and no code of a game: you need your own legally obtained copy of Fallout:
New Vegas, and the mods you use stay under their own licences. The licence covers only the original work in this
repository. Fallout and Fallout: New Vegas are trademarks of their respective owners; this project is not affiliated
with or endorsed by them.

## Licence

GPL-3.0-only (see [`LICENSE`](LICENSE)). The reason is in [`docs/DECISIONS.md`](docs/DECISIONS.md).
