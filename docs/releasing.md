# Releasing and verifying a release

release-please opens a release pull request. It is merged only after the [Release test](RELEASE-TEST.md) passes on
the owner's machine; merging it creates the tag `vX.Y.Z` and the GitHub release. `release.yml`
then dispatches `release-assets.yml`, which builds `nvmod` from that tag with `--locked` and the compiler pinned in
`rust-toolchain.toml`, and attaches:

| Asset | What it is |
| --- | --- |
| `nvmod-<v>-x86_64-linux.tar.zst` | the CLI, built on Ubuntu 22.04 (glibc 2.35 baseline); XDG paths |
| `nvmod-<v>-x86_64-windows.zip` | the CLI in the portable layout: `nvmod.portable` marker, data beside the program, no administrator rights |
| `nvmod-<v>-universal-macos.tar.zst` | the CLI for macOS, one binary for Apple silicon and Intel (arm64 and x86_64 joined with `lipo`); not code-signed yet, so Gatekeeper asks before the first run |
| `SHA256SUMS` | checksums of every other asset |
| `nvmod-<v>.cdx.json` | CycloneDX 1.5 SBOM of the `nvmod` binary |
| `release-index.json` | what `nvmod update check` reads (ADR-0010): latest version, date, notes URL, per-platform URL, SHA-256 and size, `known_good_set`, `advisories` |

Each archive holds `nvmod`, `LICENSE`, `THIRD-PARTY-NOTICES.txt` (the linked crates with licence and source) and a short
`README.txt`. Tools are never bundled. All assets carry one GitHub artifact attestation made by `release-assets.yml`.
`known_good_set` is `null` until the lock names a set id.

## Crates

The `build` and `sbom` jobs fetch crates from crates.io directly. The 7-day cooldown in `.cargo/config.toml` and
Renovate's `minimumReleaseAge` for crates are the protection; no package proxy or org secret is involved.

## Publishing

Assets are uploaded to a GitHub release only when `publish` is true, and the publish job refuses any tag that is not a
release-please release: the tag must be `vX.Y.Z`, reachable from `main`, on a `chore(main): release X.Y.Z` commit.
`release.yml` dispatches with `publish=true` for the tag it just created; a manual dispatch defaults to false.

To add the macOS archive to a release made before it existed, without a new version, dispatch with `macos_only`:
`gh workflow run release-assets.yml -R JorisJonkers-dev/nvmod-rs --ref main -f tag=vX.Y.Z -f publish=true -f macos_only=true`.
It uploads only the macOS archive, and `SHA256SUMS` and `release-index.json` made again over the release's own other
assets (the index's date becomes that day's); the OCI packages and the Toolbox notice are left out.

## Packages

A published release also pushes the two archives as OCI packages on ghcr.io:
`ghcr.io/jorisjonkers-dev/nvmod-rs:<version>-x86_64-linux` and `...:<version>-x86_64-windows`. Both archives are
checked before anything is pushed, each pushed digest gets a build provenance attestation stored in the registry, and
the job then pulls every digest anonymously and compares its SHA-256, so a package that is not public fails the
release. The package names, digests and SHA-256 sums are added to the release notes. For the first push of a new
package, which ghcr.io creates private, set the repository variable `NVMOD_FIRST_PACKAGE_PUSH` to `true` (it skips only
the anonymous pull), make the package public, then remove the variable. A pull request that changes the packaging runs
`Packages dry run`, which builds the Linux archive and prints what would be pushed.

```sh
gh attestation verify oci://ghcr.io/jorisjonkers-dev/nvmod-rs:<version>-x86_64-linux --repo JorisJonkers-dev/nvmod-rs
```

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
