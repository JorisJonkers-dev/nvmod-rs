# nvmod GUI

The nvmod GUI is a nested cargo workspace with its own `Cargo.lock`, like `fuzz/`. It is not a member of the main
workspace, so GPUI (about 560 crates, a cold build that peaks near 9 GB) never reaches the main `Cargo.lock`, the
main `cargo deny` run, Renovate noise there or the coverage gate. The reasons are in ADR-0004 of the nv-linux
repository, and the toolkit choice and its measurements are in `docs/research/gui-toolkit-spike.md` there.

**The service is the CLI subprocess service.** The GUI never links nvmod's engine crates. Every action goes through
the `Service` trait of `nvmod-ui-service`, whose real implementation runs the `nvmod` command line as a subprocess and
reads its `--json` output (ADR-0005). The window you get today runs on `FakeService`, a scripted fake, so it shows a
whole setup without touching a game folder; the real service is wired in when the screens that need it land.

## Crates

| Crate | Toolkit | What it is |
| --- | --- | --- |
| `nvmod-gui-shell` | none | Mojave Dusk and Bleach colour tokens (with the contrast test of the design table) and the setup-wizard screen as plain data, driven by `nvmod-ui-model` and any `Service`. |
| `nvmod-gui-gpui` | GPUI | The binary `nvmod-gui`: paints the shell's view and forwards presses. The only crate allowed to depend on `gpui`. |
| `nvmod-gui-egui` | egui | The canary: paints the same view from the same controller, so changing the toolkit changes only painting. It is not a second product; it builds in CI until the first real screens are done. |

`nvmod-ui-model` and `nvmod-ui-service` stay in the main workspace (their parity test runs on every pull request)
and are used from here by path.

## GPUI pins

All `gpui-pre*` crates must sit at one exact version, bumped together monthly in one commit; the 7-day minimum
package age applies. `scripts/check-gui-pins.sh` fails when the pins disagree or the lock file holds two versions.

| Crate | Pin |
| --- | --- |
| `gpui-pre` | `=0.3.11` |
| `gpui-pre-platform` | `=0.3.11` |
| `gpui-kit` | `=0.9.0` (default features off: only the facade and `gpui-base`, not the component library) |
| `eframe` (canary) | `=0.37.0` |

## Build and run

From this folder:

```sh
cargo build --release -p nvmod-gui-gpui     # target/release/nvmod-gui
cargo test --workspace                       # includes the click test through the gpui test harness
cargo run -p nvmod-gui-gpui -- --smoke=3000  # opens the window, quits by itself after 3 s (the launch check of CI)
```

A cold GPUI build needs up to about 9 GB of memory at `-j 4`. On a smaller machine lower `-j`, or take the artefact
the `GUI` workflow uploads. Linux needs the Wayland, xkbcommon, fontconfig, freetype and X11/XCB development
packages, and a Vulkan device to run (GPUI renders through wgpu over Vulkan).

### NixOS

The binary opens the Wayland client, xkbcommon and the Vulkan loader at run time, so on NixOS it must find them on
the loader path or it panics at start.

```sh
nix-shell nix/shell.nix                                    # build and run environment, LD_LIBRARY_PATH set
nix-build nix/wrapper.nix --argstr binary "$PWD/target/release/nvmod-gui"   # result/bin/nvmod-gui, wrapped
```

## CI

`.github/workflows/gui.yml` runs only when `gui/`, `nvmod-ui-model`, `nvmod-ui-service` or the workflow change, and
is separate from the main Pipeline (which skips changes under `gui/`). It is not a required check: read it before
merging a change that touches `gui/`.

| Job | What it does |
| --- | --- |
| Linux | pins check, `fmt`, `clippy -D warnings`, tests (shell logic and the click test), `cargo deny` with `gui/deny.toml` (an allow-list of licences and sources), release build of the GPUI window and the egui canary, uploads both binaries, launch check under a virtual display with a software Vulkan device (does not fail the job yet) |
| Windows | release build of the GPUI window, tests including the click test, uploads `nvmod-gui.exe`, launch check (`--smoke`, must exit cleanly) |

The gates of the spike document that CI covers: the Windows build and launch, and the click test through the
`#[gpui::test]` harness. Not covered and still to do by hand or later: a screenshot (the headless test platform
renders no pixels), the build on a 2-core, 8 GB machine, and the Orca and NVDA passes.

## Not done yet

The chamfered shapes, focus rings, key bindings, the Bleach theme switch and the Reduce motion setting of
`docs/design/gui/03-design-tokens-and-components.md`, and every screen but the setup wizard. The window uses
rounded corners of two sizes (12 and 4) as a stand-in for the chamfers.
