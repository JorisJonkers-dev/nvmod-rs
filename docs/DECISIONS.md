# Decisions

The owner delegated the open questions of the plan (`docs/plans/mod-manager.md` in NV-Linux-Modding, section 14) for
now. Each was settled with the plan's recommendation as the default, and each entry says why. **Revisit** marks the
ones the owner should look at first, because they commit money, access, scope or someone else's repository.

Date of all entries below: 2026-11-14, unless stated.

## Plan questions

| # | Question | Decision | Why | Revisit |
| --- | --- | --- | --- | --- |
| Q1 | Replace MO2, or stay next to it | **Owner: replace MO2 from the start, for the owner's own instance.** Build the full manager (import of the MO2 instance without damaging it, profiles, install, FOMOD, load order, launch). The M1 doctor stays the first deliverable. MO2 stays installed as the oracle until the two agree | Owner decision, 2026-11-14 (overrides the plan's staged default) | No |
| Q2 | Name and home | `nvmod-rs`, binary `nvmod`, under `JorisJonkers-dev`, public | Free on GitHub (no repository of that name; the closest hits are unrelated) and on crates.io (`nvmod-rs`, `nvmod` and every `nvmod-*` crate name used here returned 404 on 2026-11-14). Consistent with `nv-rs`, `loot-rs`, `xlodgen-rs` | No |
| Q3 | Licence of standalone pieces | GPL-3.0-only for everything; no dual licensing for now | `loot-rs`, which the manager will link, is GPL-3.0-only (not "or later"), so the whole binary can only be GPL-3.0-only. One licence is simpler, and nothing is reused outside yet | Only if a second consumer of `nvmod-pe` appears |
| Q4 | Nexus | Personal API key only, never SSO; offline archive install first. Whether the owner has Premium is unknown, so nothing assumes it | Nexus says public applications must register for SSO; a personal key for the owner's own use needs no registration. Not built yet (M6) | **Yes**: Premium or not, and whether the project is ever offered to others |
| Q5 | nv-rs `esm` and `bsa` crates | Not a dependency. Write a small reader here when needed, or vendor by tag later | The fork changes about 30 times a day; publishing its crates touches the fork's practices and upstream, which is the owner's call | **Yes** |
| Q6 | 4GB patch | **Owner: patch from the start.** An integrated 4GB-patch module comes early, built safely: executables are identified by content (hash plus PE header), never by name; an allow-list of input hashes; a content-addressed backup; a temporary file, verification, atomic replace and rollback; explicit confirmation before any write. Prefer `nvse_loader.exe` over the integrated Steam stub. Until the module exists the doctor only detects and advises. No game executable is ever copied into a repository | Owner decision, 2026-11-14; design in `docs/research/executables-and-4gb-patch.md` of NV-Linux-Modding | No |
| Q7 | Overlay experiment (spike S1) | **Not run.** Nothing in M1 needs it | It would mount things on the owner's machine; no permission was given in this run | **Yes**: say whether it may run, and when |
| Q8 | TTW installer | **Owner: drive external tools behind a trait**: the Linux Rust TTW installer CLI, the official installer on Windows. Pre-checks, post-verification and import are nvmod's; nothing is vendored yet | Owner decision, 2026-11-14; see `docs/research/ttw-install-automation.md` | No |
| Q9 | Windows | **Owner: Linux first, Windows follows.** Windows builds in CI from day one on hosted runners; a verified Windows backend comes after the Linux flow works. **Not applied yet in CI:** the run instructions limit CI to Linux jobs because the organisation was throttled; the Windows job is a one-job addition to `pipeline.yml` once that limit is lifted | Owner decision, 2026-11-14, against the temporary CI economy rule | **Yes**: add the Windows CI job when Actions use allows |
| Q10 | First deliverable | The M1 doctor, read-only | It carries no risk to the instance and answers "which plugins will load" | No |
| Q11 | nv-rs artefacts | **Owner: the fork will publish attested prerelease assets; nvmod pins their hashes in its own reviewed manifest as the trust root.** Not built (M5) | Owner decision, 2026-11-14 | No |
| Q12 | Linux nv-rs mode | Report only: the doctor says whether a plugin is built into nv-rs, partly, not supported or unknown. The catalogue field `nv_rs` is `unknown` everywhere because the fork's command tables were not read in this run | The Windows build under Proton (spike S5) was not run | **Yes** |
| Q13 | Components to port first beyond the spine | None. The spine order of the plan stands | Nothing outside the spine was asked for | **Yes** |

## Further owner decisions (2026-11-14, relayed)

| Topic | Decision | Notes |
| --- | --- | --- |
| UI | A full native Rust app. **Toolkit: GPUI through `gpui-component`** (packaged as `gpui-kit`), with exact version pins and the third-party republished crates, not a vendored Zed commit; **egui is the named fallback**. The GUI links the core in process through a toolkit-free view-model crate (no GPUI types in the core); the CLI keeps its own `--json` path, with a parity test over the command registry. No GUI code before the doctor and core milestones land; the first GUI step is the plan's one-day spike gates. NixOS notes (`docs/research/gui-toolkit-evaluation.md` in NV-Linux-Modding): the runtime-loaded wayland, xkbcommon and vulkan libraries need a wrapper, and lld is broken under Nix, so build GUI code with `-C link-arg=-fuse-ld=bfd`. Screen-reader support is not required; the choice is for visualisation and performance | Replaces the plan's egui default (section 9) |
| Plugin order for nv-rs | nv-rs orders plugins by file modification time and ignores `plugins.txt` order. An opt-in `plugins.txt` ordering switch is a separate task in the nv-rs fork (not in this repository). Until it exists nvmod sets file times as a fallback | Always write `plugins.txt` and `loadorder.txt` together (MO2 research, 2.12) |
| nv-rs and the patched executable | The owner's local `FalloutNV.exe` is 4GB-patched. nv-rs does **not** refuse it: its check hashes only `.rdata`, `.data`, `.tls` and `CONST` (finding of `docs/research/executables-and-4gb-patch.md`), so the line in `nv-rs-build-and-install.md` saying a patched executable is refused is wrong | The nv-rs path still never copies an executable into a repository |
| FOMOD choices | Inferred from the installed files; the owner is asked to re-run only the ambiguous ones | M2 |
| Case folding and staging | A normalised staging tree with hard links (same drive); no FUSE dependency | Overlayfs stays an option behind the `Deploy` trait |
| Oracle tests | Tests that need real game or mod data run only on the owner's machine or a private runner, never in public CI (`docs/research/private-oracle-ci/*`). Public CI uses synthetic data built in code | Already how this repository works |
| MO2 round trip | Write `modlist.txt`, `plugins.txt` and `loadorder.txt` together, MO2's formats (UTF-8 and CRLF; `plugins.txt` in the Windows 8-bit code page for FNV and TTW), atomically; never write while `ModOrganizer.exe` runs; Qt rewrites `ModOrganizer.ini`, so nvmod edits only its own keys, line by line | Readers in `nvmod-formats` follow these rules; nothing writes yet |

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
| MO2 import | Read only. Paths in `ModOrganizer.ini` are Windows paths (`Z:\home\...` under Proton); they are mapped to Linux paths for drive `Z:` and otherwise refused with a finding | The instance is on a Linux disk seen through Wine's `Z:` drive. The readers follow `docs/research/mo2-internals-and-import.md` of NV-Linux-Modding (first line highest priority, `*` foreign entries, separators, `plugins.txt` as UTF-8 or Windows-1252, directory overrides in `[Settings]`) |
| Priority order | The first line of `modlist.txt` is the highest priority and wins a file conflict | Plan requirement D1 and the update checklist |

## First run on the real instance

The doctor was run read only (headers only; no file written, nothing executed) against the owner's instance on
2026-11-14: 47 plugin DLLs in the merged view of the `TTW Wasteland Survival Guide` profile, 41 in the catalogue,
0 errors. It found the known `MAC-TEN` mismatch (`meta.ini` says Nexus id 10, the checklist says 83815), saw the
executable as 4GB-patched with a backup present and `nvse_steam_loader.dll` imported (so no loader executable is
needed to start xNVSE), and showed that **every real plugin declares itself through `NVSEPlugin_Query` only**: none
exports `NVSEPlugin_Version`. That is why the doctor does not raise a finding for the legacy interface.

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
