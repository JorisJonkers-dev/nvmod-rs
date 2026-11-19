# `nvmod nvse-usage`: extender opcode usage of an installed list

This is G-01 of the nv-rs gap analysis (`docs/research/nv-rs-gap-analysis.md` in nv-linux, issue #97). It turns the
guessed ranking of missing extender commands into a measured one.

## Do not run it on real data yet

The owner has not yet answered whether the scan may read the compiled scripts of the installed plugins (open
question 2 of the gap analysis). Until that answer is recorded, run it only on synthetic fixtures. The tests do.
Nothing in the repository or its CI runs it on game data.

## What it does

Read only. For the active plugins of one profile (`plugins.txt`), it reads the staged plugin files and extracts the
compiled script bytes (`SCDA`) of every record. It then walks the statements and expressions and counts the call
sites of every extender opcode. The source text (`SCTX`), form data and every other record content are never
returned from the reader and never reach the output. Nothing is written into the Instance or anywhere else.

The output holds only opcode numbers, command names taken from the tables you name, call counts, a status and
plugin file names. A test plants a marker string in script source and in a script string literal and asserts that
it is absent from both the table and the JSON.

Opcode blocks, by owner, as each plugin registers them (opcode base and command count in its public source, or the
`SetOpcodeBase` and `RegisterCommand` lines of its log): xNVSE `0x1400..=0x1FFF`, MCM `0x21C0..=0x21C8`, JIP LN
`0x2200..=0x233A` and `0x2600..=0x2942`, JohnnyGuitar `0x3100..=0x329B`, kNVSE `0x3920..=0x3929`, ShowOff
`0x3C93..=0x3D74` (the allocated range; the plugin registers up to `0x3D69` for scripts). Anything else from `0x2000`
up is `unknown`: other plugins hold bases between these (for example `0x21D0`, `0x3600`, `0x3961`, `0x3F40`, `0x4008`),
and a gap is never given to the plugin below it. Opcodes below `0x1400`
are the game's own and are not reported.

Status of each row:

- `built`: the command is in the installed nv-rs's `capabilities.json` (`implemented_names` of the extender `xnvse`
  or `jip-ln`).
- `missing`: the table names the command and `capabilities.json` does not list it.
- `no-table`: MCM, JohnnyGuitar, kNVSE, ShowOff and unknown blocks have no name table and no `capabilities.json` extender.
- `unchecked`: a xNVSE or JIP LN opcode that could not be judged, because `capabilities.json` is missing or invalid
  (the top-level `capabilities` field says which), or because no table named the opcode.

The count is a lower bound. Commands are found as statements, in `Set`, `If` and `ElseIf` expressions, and in
expression arguments of a command; lambda bodies and arguments of other shapes are not followed.

## Invocation

The name tables are read at run time from the nv-rs fork's files, so nothing is copied into this repository (the
licence of the vendored tables is still an open point, `docs/research/nv-rs-builtin-extenders.md` section 6).

```
nvmod nvse-usage --instance <instance dir> [--profile <name>] \
    --jip-table ~/Projects/JorisJonkers-dev/nv-rs/crates/jip/data/source-57.30.tsv \
    --nvse-names ~/Projects/JorisJonkers-dev/nv-rs/crates/nvse/src/names.rs \
    [--plugin-names <file>] [--json]
```

The Instance must be staged (`nvmod stage`). The installed nv-rs's `capabilities.json` is read from the Instance.
Without `--jip-table` and `--nvse-names` the rows of the xNVSE and JIP LN blocks have no name and are `unchecked`.

`--plugin-names <file>` names the opcodes of the blocks that have no table (MCM, JohnnyGuitar, kNVSE, ShowOff and
unknown ranges): one `opcode<TAB>name` per line, the opcode in hex with or without `0x`. Lines starting with `#`,
lines that do not parse, empty names and extra columns are ignored; opcodes of the xNVSE and JIP LN blocks keep the
names of their own tables. When a file names an opcode twice, the first name is used. A named row keeps the status `no-table`.

Exit codes follow the other commands: 0 done, 2 refused (Instance, profile, `plugins.txt` or staging tree or a
named table not readable), 5 command line not understood.

The `--json` output has `schema` (currently 1), `capabilities` (`used`, `missing` or `invalid`), `plugins_listed`,
`plugins_scanned`, `scripts_scanned` and `opcodes`, a list sorted by `calls` (most first) of
`{opcode, block, name, status, calls, plugins}`. The table prints the same columns.
