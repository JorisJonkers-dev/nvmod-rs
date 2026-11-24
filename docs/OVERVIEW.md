# nvmod in more detail

The [README](../README.md) is the short version. This page has why nvmod is its own project, what it covers,
how to work on it, and the legal notice.

nvmod is written in Rust. It keeps an Instance: the game's mods, profiles and downloads in a Mod Organizer 2 style
layout, outside the game folder.

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
  opcode usage scan ([docs/doctor-readiness.md](doctor-readiness.md), [docs/nvse-usage.md](nvse-usage.md)).

## Development

`mise install`, then `task check` runs what the pipeline runs. On NixOS without a linker:
`nix shell nixpkgs#gcc nixpkgs#lld -c cargo test` with `RUSTFLAGS="-Clink-self-contained=-linker"`. Heavy commands
go through `capped` (see [`docs/PRACTICES.md`](PRACTICES.md)). Test files are built in code by `nvmod-testkit`;
no game or mod file is ever committed. Decisions are one file each in [`docs/decisions/`](decisions/README.md);
earlier ones are in [`docs/DECISIONS.md`](DECISIONS.md).

## Legal notice

nvmod contains no game files, no mod files and no code of a game: you need your own legally obtained copy of Fallout:
New Vegas, and the mods you use stay under their own licences. The licence covers only the original work in this
repository. Fallout and Fallout: New Vegas are trademarks of their respective owners; this project is not affiliated
with or endorsed by them.

## Licence

GPL-3.0-only (see [`LICENSE`](../LICENSE)). The reason is in [`docs/DECISIONS.md`](DECISIONS.md).
