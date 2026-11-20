# Run a modded game natively on Linux

`nvmod run` starts nv-rs against the merged Data folder of a profile. No game file is copied and the Steam folder is
only read.

> **Moving to the Toolbox.** Starting programs, the engine install and setup move to
> [rauls-trusty-toolbox](https://github.com/JorisJonkers-dev/rauls-trusty-toolbox) (its ADR-0007 and `docs/REORG.md`).
> After the move, `nvmod run` and `nvmod install-nv-rs` are removed and `raul` does these steps; the steps
> below that only manage mods (init, install, import, profiles, sort) stay in nvmod.

## Steps

1. `nvmod init --base <game folder> --instance <dir>` and set the target: `nvmod target nv-rs --instance <dir>`.
2. Install or import your mods and make a profile (`nvmod import-mo2`, `nvmod install`, `nvmod profile create`).
3. Order the plugins: `nvmod sort --instance <dir> --masterlist <file> --apply`. nv-rs orders by file time and this
   sets the times.
4. Name the program to start with `--viewer` (raul does).
5. See the plan: `nvmod run nv-rs --instance <dir> --dry-run`. It names the viewer, the run folder, the user data, the
   environment and the log, and writes nothing.
6. Start: `nvmod run nv-rs --instance <dir> --wait` (without `--wait` the Instance stays locked until the game ends).

On an Instance whose target is `nv-rs`, `nvmod run game` means the same. `--via-proton` runs `game` through Proton
instead, as a Vanilla Instance does.

## What the run builds

`<instance>/run/<profile>/` holds `Data` (a link to the Staging tree), `FalloutNV.exe` and `Fallout_default.ini` (links
to the base copy), `linux_profile.txt` and `userdata/`. The profile's INIs, `plugins.txt` and `loadorder.txt` are
placed under `userdata/`; what the game changes there is collected back by `nvmod profile finish` (run with `--wait`,
it is done for you). A profile without a `fallout.ini` gets one from `Fallout_default.ini` on the first start.

## Options and environment

- `--viewer <path>` or `NV_VIEWER`: the program to start. Nothing is searched for: without it the run is refused
  (raul always names its own program).
- `NV_D3D9`, `RUST_LOG` and `RUST_BACKTRACE` default to `wgpu`, `info` and `1`; set them to override.
- Output goes to `~/.local/state/nv-rs/logs/run-<UTC time>.log` (`$XDG_STATE_HOME` is honoured). The first lines say
  which build, profile and folder were used. Follow it with `tail -f`.
- The wayland, xkbcommon and vulkan libraries are loaded at run time; where the system has none (Nix), put them on
  `LD_LIBRARY_PATH` yourself.

Plugins that are external NVSE DLLs are not loaded by nv-rs; `nvmod doctor` reports which ones are built in.
