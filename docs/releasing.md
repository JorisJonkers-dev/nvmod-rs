# Releasing and verifying a release

release-please opens a release pull request; merging it creates the tag `vX.Y.Z` and the GitHub release. `release.yml`
then calls `release-assets.yml`, which builds `nvmod` from that tag with `--locked` and the compiler pinned in
`rust-toolchain.toml`, and attaches:

| Asset | What it is |
| --- | --- |
| `nvmod-<v>-x86_64-linux.tar.zst` | the CLI, built on Ubuntu 22.04 (glibc 2.35 baseline); XDG paths |
| `nvmod-<v>-x86_64-windows.zip` | the CLI in the portable layout: `nvmod.portable` marker, data beside the program, no administrator rights |
| `SHA256SUMS` | checksums of every other asset |
| `nvmod-<v>.cdx.json` | CycloneDX 1.5 SBOM of the `nvmod` binary |
| `release-index.json` | what `nvmod update check` reads (ADR-0010): latest version, date, notes URL, per-platform URL, SHA-256 and size, `known_good_set`, `advisories` |

Each archive holds `nvmod`, `LICENSE`, `THIRD-PARTY-NOTICES.txt` (the linked crates with licence and source) and a short
`README.txt`. Tools are never bundled. All assets carry one GitHub artifact attestation made by `release-assets.yml`.
`known_good_set` is `null` until the lock names a set id.

## Verify

```sh
sha256sum -c SHA256SUMS --ignore-missing
gh attestation verify nvmod-<v>-x86_64-linux.tar.zst -R JorisJonkers-dev/nvmod-rs \
  --signer-workflow JorisJonkers-dev/nvmod-rs/.github/workflows/release-assets.yml
```

## Unsigned Windows build

The Windows binary is not code-signed (signing is a spend the owner has not made). SmartScreen may warn on first run,
and antivirus heuristics may flag a program that can write a 4 GB-patched game executable. nvmod never copies a game
executable into a repository. The checksum and the attestation are the integrity check.

## Dry run

Dispatch `Release assets` on the Actions tab with an existing tag and `publish` left false: it builds, attests and keeps
the files as the `release-assets-dry-run` workflow artifact, without touching the release.
