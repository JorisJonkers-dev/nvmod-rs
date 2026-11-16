# Doctor readiness on the nv-rs target

`nvmod doctor --instance <dir>` on an Instance whose target is nv-rs ends its report with a readiness block, and the
`--json` output has the same data under `nv_rs.readiness`. It is computed from the catalogue (`category` and
`provided_by.nv-rs` of each entry), the installed nv-rs's `capabilities.json` and the profile's Staging tree and
`modlist.txt`. It reads nothing else and prints no mod names.

`nv_rs.readiness`:

- `categories`: one row per `script-extender`, `render`, `patch`, `lod`, then `unclassified` when a DLL is not in the
  catalogue or the entry has no category. Each row: `present` (plugin DLLs in the Staging tree), `built_in` (built into
  nv-rs with no command missing), `partial` (built in with commands still missing), `external` (nothing records that
  nv-rs loads it), `absent` (cannot load on nv-rs) and `ready_percent` (`built_in` of `present`, left out when none).
- `mods`: the enabled mods by whether their folder holds a DLL: `data_only`, `with_dlls`, `missing` (folder not found).
- `commands`: `implemented`, `total` and `percent` summed over every extender in `capabilities.json`; left out when the
  file is missing or unusable. It rises when nv-rs lists a new command.

The categories are the catalogue's, recorded from the plugins' roles and unverified; a wrong one moves a count between
rows, never a finding.
