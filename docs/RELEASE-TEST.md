# Release test

A release pull request is merged only after this test passes on the owner's machine, against the owner's own
Instance. The results go in the release pull request as a comment, as numbers and verdicts only: no paths, mod names
or plugin names.

## Checklist

1. **Build the release commit.** Check out the head of the release pull request in its own worktree and build it:
   `cargo build --release --locked -p nvmod-cli`. `nvmod --version` prints the release's version.
   Record: the commit, the build time, the version printed.
2. **Doctor.** `nvmod doctor --instance <dir> --json` on the owner's Instance finishes and lists its findings. The
   Doctor is read only; nothing in the Instance changes. Exit 0 (nothing found) and 1 (problems found) both count as
   finished; any other exit fails the step.
   Record: the exit code, the number of findings by severity, the run time.
3. **Run.** `nvmod run nv-rs --instance <dir> --wait` starts nv-rs in a nested headless compositor with the VRAM
   watchdog on, and reaches the main menu.
   Record: which nv-rs build ran (a release of the current Known-good set, or the newest local build if that is what
   `nvmod run` picks), the verdict, the time to the main menu, the peak VRAM, the exit code.
4. **Rolling engine channel: not part of nvmod.** The compatibility check and pointer rollback of newer nv-rs builds
   moved to the Toolbox (rauls-trusty-toolbox ADR-0011) and are tested there. This step is recorded as "moved to the
   Toolbox".

The release passes when steps 1 to 3 pass.

## How to run it

### Step 2

The Instance is read, never written. Keep a timestamp file and confirm afterwards that `find <instance> -newer <file>`
prints nothing.

### Step 3

`nvmod run` writes a run folder into the Instance it starts (`run/<profile>/`) and collects the profile's files back
at the end. To leave the owner's Instance untouched, run it on a side Instance: a new folder with copies of
`nvmod-instance.json`, `profiles/` and `.nvmod/`, and symbolic links to the real `mods/` and `staging/`. Pass
`--no-stage` so the linked Staging tree is only read. `nvmod run nv-rs --instance <side> --no-stage --dry-run` shows
which nv-rs build it will start.

Game runs on the owner's machine follow the fork's bench rules (nv-rs `docs/fork/BENCH.md`):

- Take the machine-wide bench lock for the whole in-game part, as `nv-rs-dev bench run` does
  (`scripts/fork/bench/lock.py`, `BenchLock`).
- Run the game only in a nested headless sway session (`scripts/fork/bench/nested.py`, mode `headless`), never in the
  owner's desktop session. Remove `DISPLAY` and the session's `WAYLAND_DISPLAY` from the environment, then add the
  nested session's.
- Keep the VRAM watchdog on at 50% (`NV_VRAM_LIMIT_PCT=50`; `Watchdog` in `scripts/fork/bench/runner.py` samples the
  card once a second and stops the game at the limit). Sound goes to the null sink (`NV_SOUND=out`,
  `NV_SOUND_BACKEND=null`).
- Write the frame log (`NV_FRAME_LOG`). Stop the game by its exact process id.

The main menu is decided from the frame log, as the bench's `boot` scenario does: the first frame comes, the start-up
movies are skipped with Escape, and the screen column then stays `menu` for 8 seconds. To confirm it is the main menu
and not a movie, send the keys of the bench's `newgame` driver (Down, Down, Enter, Enter) and wait for the `loading`
screen; stop the game as soon as it appears.

The start-up movies also show as `menu` in the frame log, so time to the main menu is counted from the start of
`nvmod run` to the moment the 8-second check after the skipped movies passes. Peak VRAM is the highest card-wide VRAM
use the watchdog sampled, with the use before the start named beside it. When the game stops before the first
sample, write "not sampled".

If step 3 fails, find out whether nvmod or nv-rs is the cause: repeat it on a new vanilla Instance (`nvmod init`,
`nvmod profile create`, `nvmod target nv-rs`, `nvmod stage`). If that run reaches the main menu, the failure is in
nv-rs and its stop message goes in an nv-rs issue.
