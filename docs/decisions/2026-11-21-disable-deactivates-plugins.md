# Disabling a Mod takes its own plugins out of the active list (#257)

- **Question:** after `nvmod mods disable`, `plugins.txt` still listed the plugins only that Mod provided as active,
  so the Load order and the Doctor named plugins nothing staged any more.
- **Decision:** a disable takes every plugin that the disabled Mods provide at the top of their folder (`.esp`,
  `.esm`, `.esl`) and that no still-enabled Mod provides out of `plugins.txt`. `loadorder.txt` is not touched, so the
  place of the plugin is kept. The report lists them as `plugins_deactivated` (absent when empty) and the text output
  names them. The journal of a Mods change now restores `plugins.txt` as well as `modlist.txt`, so `nvmod rollback`
  brings the active list back. Enabling a Mod does not activate anything; `nvmod plugins enable` does, and installing
  a Mod still lists its plugins.
- **Why:** the Doctor checks only the active plugins against the Load order, so dropping them from the active list
  makes it say the truth, while keeping the line in `loadorder.txt` keeps the position for a later enable. The other
  option, removing the lines from both lists as `mods remove` does, loses the position; activating plugins on every
  enable would also undo a plugin the user switched off on purpose.
- **Revisit:** if users expect a re-enabled Mod to bring its plugins back active.
