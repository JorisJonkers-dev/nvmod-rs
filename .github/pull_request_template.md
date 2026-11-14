## Change

What this pull request changes and why.

## Checklist

- [ ] No game data and no mod files: nothing with an extension in `scripts/repo_guards.py`, no extract, dump or
      screenshot from real data. Test files are built in code (`crates/nvmod-testkit`).
- [ ] Every parser of untrusted input reads through bounded readers and has a fuzz target in `fuzz/`.
- [ ] Every catalogue entry or format fact carries its provenance (`docs/PRACTICES.md`).
- [ ] New source files carry `SPDX-License-Identifier: GPL-3.0-only` in their first five lines.
- [ ] Checks: `task check` passes locally.

## Verification

What was run and what it showed.
