# Disable never touches base plugins, enable puts the plugins back (#261)

Amends `2026-11-21-disable-deactivates-plugins.md` (#257).

- **Question:** disabling Tale of Two Wastelands emptied `plugins.txt` (base masters included) and enabling it again
  brought nothing back. What should a disable take out, what should an enable restore, and what happens to plugins of
  other Mods whose master came from the disabled Mod?
- **Decision:**
  1. A disable takes out only plugins that no enabled Mod provides **and the base copy does not have** (its known base
     masters and every plugin in its `Data` folder). A Mod may ship its own copy of a base master (Tale of Two
     Wastelands does for `FalloutNV.esm`, the DLC and the packs); those lines stay.
  2. Both steps keep every other byte of `plugins.txt`. Each line a disable takes out is noted in the Profile's
     `deactivated.txt` (the line above it, the line, its line end). An enable puts back every noted plugin that the
     enabled Mods provide, below the same line as before; when that line is gone it goes after the nearest earlier
     plugin of `loadorder.txt` that is listed, else to the end. A plugin that is listed already is not added twice.
     So disable then enable is byte-identical on `plugins.txt`, and `loadorder.txt` is never touched.
  3. `deactivated.txt` is part of the journal of a Mods change, so `nvmod rollback` undoes either step including the
     notes. The report gains `plugins_reactivated` (absent when empty).
  4. Plugins of **other** Mods whose masters come from the disabled Mod stay listed and active. The Doctor already
     reports each as `loadorder.missing-master` ("its master X is in the Load order but not active"); the enable
     clears the finding again. Nothing is deactivated on their behalf.
- **Why:** the disable should do one thing, name only what it changed, and be reversible exactly. Deactivating the
  dependents would need a second set of notes, would also drop plugins the user wants to keep for a later enable of a
  different provider, and hides the problem the Doctor is meant to show. The cost is that the game would start with a
  missing master until the Mod is back or the dependent is deactivated by hand; the Doctor says so and the Toolbox
  shows it.
- **Revisit:** if the Toolbox wants a one-click "deactivate what depends on it" on the Mods page (a separate command,
  not part of the disable).
