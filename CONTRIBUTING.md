# Contributing

This project is licensed GPL-3.0-only (see `LICENSE`) and owned by Joris Jonkers.

External contributions are not accepted unless Joris Jonkers explicitly asks
for them. Public pull requests may be closed without review.

If you were invited to contribute:

1. keep changes scoped to the requested repository and issue
2. do not include secrets, private data, local scratch files, or generated
   planning artifacts
3. do not hand-edit `CHANGELOG.md`; release-please owns changelog updates
4. record a new decision as one new file `docs/decisions/<YYYY-MM-DD>-<slug>.md` (never edit `docs/DECISIONS.md`)
5. include the relevant tests or validation output in the pull request
6. use Conventional Commits for every commit and pull request title (`scripts/install-git-hooks.sh`
   installs the local check), with impersonal, professional wording

Security vulnerabilities must be reported privately as described in
`SECURITY.md`.
