# Engine install removed (#168)

- **Question:** does nvmod keep installing the engine, now that the Toolbox ships it?
- **Decision:** `nvmod install-nv-rs`, its job (`nvmod_api::install_nvrs`) and the crate `nvmod-install` are gone; the command is an unknown command (exit 5). `nvmod verify` still reads `nv-rs/install.json` and the Doctor still reads `nv-rs/capabilities.json` as data. The launch part of `nvmod-run` (`launch.rs`, the `Launcher` seam and `ProcessLauncher`) stays: the Toolbox's play mode links it (`nvmod_run::{LaunchSpec, Launcher, Running, ProcessLauncher}`, `nvmod_cli::run_with_launcher`).
- **Why:** nothing in the Toolbox calls the engine install (checked at its main); the launch seam is still used, so it cannot go before a Toolbox change.
- **Revisit:** no.
