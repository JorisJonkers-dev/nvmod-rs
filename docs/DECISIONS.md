# Decisions

The owner delegated the open questions of the plan (`docs/plans/mod-manager.md` in NV-Linux-Modding, section 14) for
now. Each was settled with the plan's recommendation as the default, and each entry says why. **Revisit** marks the
ones the owner should look at first, because they commit money, access, scope or someone else's repository.

Date of all entries below: 2026-11-14, unless stated.

## Plan questions

| # | Question | Decision | Why | Revisit |
| --- | --- | --- | --- | --- |
| Q1 | Replace MO2, or stay next to it | Replace it for this instance, in stages: read-only doctor (M1), installer (M2), then sort, LOD, nv-rs install, launch. MO2 stays the oracle until the harness says both agree | M1 and M2 are useful either way; only M3 onwards depends on the answer | **Yes**, before M3 |
| Q2 | Name and home | `nvmod-rs`, binary `nvmod`, under `JorisJonkers-dev`, public | Free on GitHub (no repository of that name; the closest hits are unrelated) and on crates.io (`nvmod-rs`, `nvmod` and every `nvmod-*` crate name used here returned 404 on 2026-11-14). Consistent with `nv-rs`, `loot-rs`, `xlodgen-rs` | No |
| Q3 | Licence of standalone pieces | GPL-3.0-only for everything; no dual licensing for now | `loot-rs`, which the manager will link, is GPL-3.0-only (not "or later"), so the whole binary can only be GPL-3.0-only. One licence is simpler, and nothing is reused outside yet | Only if a second consumer of `nvmod-pe` appears |
| Q4 | Nexus | Personal API key only, never SSO; offline archive install first. Whether the owner has Premium is unknown, so nothing assumes it | Nexus says public applications must register for SSO; a personal key for the owner's own use needs no registration. Not built yet (M6) | **Yes**: Premium or not, and whether the project is ever offered to others |
| Q5 | nv-rs `esm` and `bsa` crates | Not a dependency. Write a small reader here when needed, or vendor by tag later | The fork changes about 30 times a day; publishing its crates touches the fork's practices and upstream, which is the owner's call | **Yes** |
| Q6 | 4GB patch | Detect and advise only (large-address-aware flag, backup present). Never patch | It edits a Steam-distributed executable | Later, if wanted |
| Q7 | Overlay experiment (spike S1) | **Not run.** Nothing in M1 needs it | It would mount things on the owner's machine; no permission was given in this run | **Yes**: say whether it may run, and when |
| Q8 | TTW installer | Import its output only. `nvmod` does not run an installer yet | Which installer the owner trusts is unknown | **Yes** |
| Q9 | Windows | The core is platform neutral; CI builds and tests on Linux only | Actions minutes are limited (the organisation was throttled); nothing in M1 is Windows specific. The Windows build is untested, which the README states | Yes, when a Windows run is wanted |
| Q10 | First deliverable | The M1 doctor, read-only | It carries no risk to the instance and answers "which plugins will load" | No |
| Q11 | nv-rs artefacts | Fetch and verify is the intended default (M5). Not built | Needs a token and a decision on retention | **Yes**, before M5 |
| Q12 | Linux nv-rs mode | Report only: the doctor says whether a plugin is built into nv-rs, partly, not supported or unknown. The catalogue field `nv_rs` is `unknown` everywhere because the fork's command tables were not read in this run | The Windows build under Proton (spike S5) was not run | **Yes** |
| Q13 | Components to port first beyond the spine | None. The spine order of the plan stands | Nothing outside the spine was asked for | **Yes** |

## Implementation decisions

| Topic | Decision | Why |
| --- | --- | --- |
| PE parser | Hand written (`nvmod-pe`), not `goblin` | The parser reads through a bounded reader, sizes no buffer from a header field beyond fixed limits and the file length, and never maps the whole file. It needs about ten fields. It is fuzzed. Approach taken from `loot-rs` `src/exe_version.rs` (read there, not copied: that code is for another purpose) |
| Command line | Hand written argument handling, no `clap` | Three commands and a handful of flags; a smaller dependency tree and no proc-macro build cost. Move to `clap` when the commands multiply |
| Repository agent files | No `CLAUDE.md` or `AGENTS.md` | They would only restate the estate pointers; add them if the estate tooling needs them |
| Releases | `release-type: simple` with `release-as: 0.1.0` | As in `lodwright-rs`: one product, one version, one tag. Remove `release-as` after the first release |
| Merges | Squash only, auto-merge, branch deleted after merge | The owner's instruction; the PR title becomes the commit, so it must be a Conventional Commit |
| CodeQL | Default setup for `actions` and `python` | Rust is not offered by default setup. The Python in the repository is the guard script |
| Catalogue source | The DLL and INI names in `data/nvse-plugins.toml` were read from the file names in the owner's MO2 instance on 2026-11-14 (names only, nothing opened); Nexus ids come from the update checklist in NV-Linux-Modding. Everything else (minimum xNVSE, requirements, conflicts) is empty or `unknown` | Provenance per entry; nothing is invented |
| MO2 import | Read only. Paths in `ModOrganizer.ini` are Windows paths (`Z:\home\...` under Proton); they are mapped to Linux paths for drive `Z:` and otherwise refused with a finding | The instance is on a Linux disk seen through Wine's `Z:` drive. `docs/research/mo2-internals-and-import.md` had not landed when this was written; the readers follow the real profiles in `mo2/` of NV-Linux-Modding. Reconcile when it lands |
| Priority order | The first line of `modlist.txt` is the highest priority and wins a file conflict | Plan requirement D1 and the update checklist |

## Unverified assumptions

These are in the code as named constants or comments and in findings as lower certainty. Each is a spike to close
against the real xNVSE source and a real `nvse.log`.

1. **Version struct layout.** The doctor treats the export `NVSEPlugin_Version` as a data symbol that starts with two
   little-endian `u32`: `infoVersion` and `version`. Nothing after them is read. The layout is from memory (plan
   spike S2). The finding says so.
2. **Which exports make an NVSE plugin.** `NVSEPlugin_Query`, `NVSEPlugin_Load` and `NVSEPlugin_Version` (plan
   section 6.1, also from memory). A DLL with none of them is reported as "not an NVSE plugin", which can be right
   for a helper library placed in the plugin folder.
3. **Supported game executable.** `1.4.0.525` (plan section 6.1 item 2).
4. **Runtime DLL names** treated as "needs the Visual C++ runtime in the prefix": `msvcp*`, `vcruntime*`, `concrt*`
   (versions 100 and up; `msvcr*` of the 2010 and earlier runtimes is reported as a runtime too).
