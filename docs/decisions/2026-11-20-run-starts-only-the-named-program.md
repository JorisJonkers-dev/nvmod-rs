# Run starts only the named program (#241)

- **Question:** does `nvmod run` look for a separate engine program when none is named?
- **Decision:** `nvmod run nv-rs` (and `game` on an nv-rs Instance) starts the program named with `--viewer` or `NV_VIEWER`; it no longer looks in the Instance's `nv-rs/bin` or under `~/.local/opt/nv-rs` (`NVMOD_NV_RS_ROOT` and the `shared_root` field of the run request are gone). Without one the refusal says no program is named and names no command.
- **Why:** raul links the engine and always passes its own program; there is no separate engine program or install step (Toolbox #378, #432).
- **Revisit:** no.
