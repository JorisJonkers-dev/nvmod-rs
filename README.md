# nvmod-rs

**nvmod installs and manages mods for Fallout: New Vegas and Tale of Two Wastelands**, on Linux, Windows and macOS.
Your game folder stays as it is: mods live in a folder of their own, and the game sees them when you play.

<p align="center">
  <img src="docs/images/how-it-works.svg" alt="A downloaded mod goes into nvmod-rs's own folder and the game sees it when you play; the game folder stays untouched" width="100%">
</p>

## What you see

nvmod is the **Mods** page of [raul](https://github.com/JorisJonkers-dev/rauls-trusty-toolbox): your mods in a list,
and a mod's installer asks its questions in the window.

<p align="center">
  <img src="https://raw.githubusercontent.com/JorisJonkers-dev/rauls-trusty-toolbox/main/crates/raul-gui/tests/snapshots/mods/mods-listed.png" alt="raul's Mods page: the mods with their priority and the selected mod's actions" width="49%">
  <img src="https://raw.githubusercontent.com/JorisJonkers-dev/rauls-trusty-toolbox/main/crates/raul-gui/tests/snapshots/mods/mods-install-questions.png" alt="Installing a mod with an installer: its questions in raul's window" width="49%">
</p>

## What it does for you

- 📦 **Installs mods** from their .zip or .7z files, installers included.
- 🔀 **Turn mods on and off**, change which one wins, remove them, and undo any change.
- ⚠️ **Finds problems**: two mods that change the same file, or a plugin that misses another one it needs.
- 💾 **Backs up** your profiles and saves, and moves an existing Mod Organizer 2 setup over.

<!-- Progress picture: docs/coverage/tally.svg goes here once the feature catalogue reaches this repository. -->

## How it fits together

<p align="center">
  <img src="docs/images/raul-parts.svg" alt="raul and its five parts: nv-rs the game engine, nvmod-rs mods (this project), loot-rs load order, lodwright-rs distant view, ttwright-rs Tale of Two Wastelands" width="100%">
</p>

nvmod is one part of raul and only manages mods; raul runs the other steps. As its own project it is tested on its own.

## Get it

**[Download raul](https://github.com/JorisJonkers-dev/rauls-trusty-toolbox/releases/latest)**: nvmod is built in.
It also works on its own from the command line ([docs/USAGE.md](docs/USAGE.md)).

## Learn more

- [Using nvmod](docs/USAGE.md): raul's Mods and Load order pages, the commands and the downloads.
- [More detail](docs/OVERVIEW.md): what it covers, working on it, the legal notice.

nvmod contains no game or mod files: you need your own copy of the game. Licence: GPL-3.0-only ([LICENSE](LICENSE)).
