# Decisions

Each new decision is ONE NEW FILE: `docs/decisions/<YYYY-MM-DD>-<slug>.md`. There is no running number, so two pull
requests that each record a decision never touch the same file and never conflict.

`docs/DECISIONS.md` is frozen history. Never edit it again: `python3 scripts/repo_guards.py decisions` (part of
`task guards`) fails when it grows.

## File format

The same fields a row of the old table had:

```markdown
# <short title> (<issue or PR>)

- **Question:** what had to be settled.
- **Decision:** what was chosen, concretely enough to check against the code.
- **Why:** the reason, and what was given up.
- **Revisit:** when or on what evidence this should be looked at again, or `no`.
```

Use the date the decision was made and a short lowercase slug (`2026-11-21-overwrite-sync-copies.md`). Longer
decisions may add sections under the fields. Amending an earlier decision is a new file that names the one it changes.
