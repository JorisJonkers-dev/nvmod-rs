# Using nvmod

The [README](../README.md) is the short version. This page has how raul uses nvmod, its own commands and its
downloads.

## Used from raul

raul's **Mods** page is nvmod: install a mod from its archive, answer a FOMOD installer's questions in the window,
turn mods on and off, change their priority, remove them, and undo any of it. raul takes an automatic snapshot of the
profile before an install or a change to the list.

<p align="center">
  <img src="https://raw.githubusercontent.com/JorisJonkers-dev/rauls-trusty-toolbox/main/crates/raul-gui/tests/snapshots/mods/mods-listed.png" alt="raul's Mods page: the mods with their priority and the selected mod's actions" width="49%">
  <img src="https://raw.githubusercontent.com/JorisJonkers-dev/rauls-trusty-toolbox/main/crates/raul-gui/tests/snapshots/mods/mods-install-questions.png" alt="Installing a mod with an installer: its questions in raul's window" width="49%">
</p>

The **Load order** page lists the plugins and sorts them with nvmod's sort, which uses
[loot-rs](https://github.com/JorisJonkers-dev/loot-rs) and the LOOT masterlist. raul shows the moves and the
masterlist's messages before anything changes. The Home page's load order check runs nvmod's Doctor.

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
artifact attestation; see [docs/releasing.md](releasing.md).
