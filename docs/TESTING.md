# Testing

`task check` runs what the pipeline runs (see [PRACTICES.md](PRACTICES.md)). Tests build every file they need in code
with `nvmod-testkit`; no game or mod file is ever committed. Heavy local runs go through `capped`.

## Feature catalogue

`catalogue/` lists what nvmod promises: the areas (Instance, Mods, Staging tree, Plugins, Doctor, backups and
import, running programs, external tools, the library and command line, file readers), their groups (one TOML file
each) and the items in them, with a state (`implemented`, `missing`, `deferred`, `n/a`; `gated` marks an item that
needs something CI does not have). The format is tally's (<https://github.com/JorisJonkers-dev/tally>,
`docs/FORMAT.md`). The catalogue follows no upstream program, so it has no reference lists.

Tests say which items they prove, and only what they assert on:

```rust
use nvmod_testkit::{covers, ids};

#[test]
fn a_clean_instance_exits_0_and_prints_the_doctor_report_and_the_launch_plan() {
    covers(&[ids::doctor::verify::CLEAN]);
    // ...
}
```

`crates/nvmod-testkit/build.rs` writes the `ids` module from the catalogue (through `catalogue/tally_ids_gen.rs`, a
copy of tally's standard-library-only generator, so no build dependency), so a wrong or removed id does not compile.
Every crate's tests reach it through `nvmod-testkit`. To change an item's state edit its catalogue file; to rename
one, keep the old id in `aliases`; an item that goes away keeps its entry with `retired`.

The pipeline step "Feature catalogue (tally)" of Pipeline Complete runs the pinned tally release: ids valid and
unique, the generator copy current, tags name real ids from inside tests, and no id of the base revision disappears
without `retired` or an alias.

Tests run through cargo-nextest with the `ci` profile (`.config/nextest.toml`; `task test` on Linux, the platforms job on
Windows and macOS), with no retries and every skipped test listed, so each run leaves a JUnit file. The step reads the
three files (`tally ingest`): an item is **covered** once its tagged tests passed on all three systems. With those
results it holds a ratchet: the covered and tagged counts may not drop below the report of the base revision
(`docs/coverage/tally-report.json` on main), so a pull request that loses a tag or a system fails. A drop that is
meant (an item retired or set `n/a`) needs `--allow-drop <n>` on that step in the same pull request, removed
again afterwards.

On main, every push builds and tests on all three systems; the step then renders `docs/coverage/FEATURES.md`,
`docs/coverage/tally-report.json` and `docs/coverage/tally.svg`, and when one changed and every job passed, the job
"Catalogue reports" commits only those files as raul-progress-bot with the skip-ci marker. A pull request that
changes an existing file of `docs/coverage/` fails: those files are written on main only. The marker anywhere in a squash-merge message
skips main's run for that commit too, so keep it out of pull request titles and bodies.

Locally, with tally installed: `tally check`, `tally report --depth 2`, and `tally ingest --junit
target/nextest/ci/junit.xml --out results.json` after `cargo nextest run --profile ci -p <crate>`, then
`tally report --results results.json`.
