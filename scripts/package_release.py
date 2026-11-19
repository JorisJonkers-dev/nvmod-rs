#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Release packaging helpers for .github/workflows/release-assets.yml. Standard library only.

    package_release.py notices --metadata FILE --out FILE
    package_release.py package --target {linux,windows,macos} --version V --binary FILE --notices FILE --out DIR
    package_release.py sums    --dir DIR --out FILE
    package_release.py index   --dir DIR --tag TAG --repo OWNER/REPO --date YYYY-MM-DD [--set-id ID] --out FILE

`notices` lists the crates that the `nvmod` binary links (from `cargo metadata --locked`), with licence and source.
`package` builds nvmod-<version>-x86_64-linux.tar.zst, nvmod-<version>-x86_64-windows.zip (ADR-0009) or
nvmod-<version>-universal-macos.tar.zst (arm64 and x86_64 in one binary, joined with lipo). Nothing but
nvmod, the licence and the notices is packaged: tools are fetched by lock, never bundled. The Windows zip carries
the `nvmod.portable` marker (data beside the program, no administrator rights); the tarballs keep the system's own
data folders.
`index` writes release-index.json (ADR-0010) from the archives found in DIR.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tarfile
import zipfile
from pathlib import Path

BINARY_PACKAGE = "nvmod-cli"
ROOT = Path(__file__).resolve().parent.parent
INDEX_SCHEMA_VERSION = 1
# The processor part of each target's archive name: the macOS binary holds both arm64 and x86_64.
ARCH = {"linux": "x86_64", "windows": "x86_64", "macos": "universal"}
ARCHIVE_RE = re.compile(
    r"^nvmod-(?P<v>\d+\.\d+\.\d+[^-]*)-(?P<a>x86_64|universal)-(?P<p>linux|windows|macos)\.(tar\.zst|zip)$"
)
README_TEXT = """nvmod {version}

nvmod installs and manages a modded Fallout New Vegas / TTW setup on nv-rs. It never bundles game data or
tools: tools are fetched by the version lock and verified (checksum and build attestation) before they run.

Licence: GPL-3.0-only, see LICENSE. The licences of the linked crates are in THIRD-PARTY-NOTICES.txt.
Source of this release: https://github.com/JorisJonkers-dev/nvmod-rs/tree/v{version}
{portable}"""
PORTABLE_TEXT = """
This is the portable layout: the empty file nvmod.portable next to nvmod.exe keeps the data in a folder beside
the program. Extract the zip to a folder of your own; no administrator rights are needed. The program is not
code-signed yet, so Windows SmartScreen may warn on first run. Check the download against SHA256SUMS (or with
`gh attestation verify`) before you run it.
"""


def reachable_packages(metadata: dict) -> list[dict]:
    """Packages that BINARY_PACKAGE links: normal dependencies only (no dev or build-only edges)."""
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    root = next(p["id"] for p in metadata["packages"] if p["name"] == BINARY_PACKAGE)
    seen, stack = set(), [root]
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    return sorted((packages[i] for i in seen), key=lambda p: (p["name"], p["version"]))


def notices_text(metadata: dict) -> str:
    lines = [
        "Third-party notices for nvmod.",
        "",
        "These crates are linked into the nvmod binary (normal dependencies, from Cargo.lock).",
        "The licence text of each is in its source package, at the repository named below.",
        "",
    ]
    for pkg in reachable_packages(metadata):
        if pkg["name"].startswith("nvmod-"):
            continue
        licence = pkg.get("license") or pkg.get("license_file") or "UNKNOWN"
        lines.append(f"{pkg['name']} {pkg['version']}")
        lines.append(f"  licence: {licence}")
        lines.append(f"  source: {pkg.get('repository') or pkg.get('homepage') or 'crates.io'}")
        lines.append("")
    return "\n".join(lines)


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def stage(target: str, version: str, binary: Path, notices: Path, work: Path) -> Path:
    portable = target == "windows"
    folder = work / f"nvmod-{version}-{ARCH[target]}-{target}"
    folder.mkdir(parents=True)
    exe = "nvmod.exe" if portable else "nvmod"
    (folder / exe).write_bytes(binary.read_bytes())
    (folder / exe).chmod(0o755)
    (folder / "LICENSE").write_bytes((ROOT / "LICENSE").read_bytes())
    (folder / "THIRD-PARTY-NOTICES.txt").write_bytes(notices.read_bytes())
    (folder / "README.txt").write_text(
        README_TEXT.format(version=version, portable=PORTABLE_TEXT if portable else ""), newline="\n"
    )
    if portable:
        (folder / "nvmod.portable").write_bytes(b"")
    return folder


def package(target: str, version: str, binary: Path, notices: Path, out: Path) -> Path:
    import tempfile

    out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        folder = stage(target, version, binary, notices, Path(tmp))
        files = sorted(p for p in folder.iterdir() if p.is_file())
        if target == "windows":
            archive = out / f"{folder.name}.zip"
            with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zf:
                for path in files:
                    info = zipfile.ZipInfo(f"{folder.name}/{path.name}", (1980, 1, 1, 0, 0, 0))
                    info.compress_type = zipfile.ZIP_DEFLATED
                    info.external_attr = (0o644 if path.name != "nvmod.exe" else 0o755) << 16
                    zf.writestr(info, path.read_bytes())
            return archive
        tar_path = Path(tmp) / f"{folder.name}.tar"
        with tarfile.open(tar_path, "w", format=tarfile.PAX_FORMAT) as tf:
            for path in files:
                info = tf.gettarinfo(str(path), arcname=f"{folder.name}/{path.name}")
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                info.mtime = 0
                info.mode = 0o755 if path.name == "nvmod" else 0o644
                with path.open("rb") as handle:
                    tf.addfile(info, handle)
        archive = out / f"{folder.name}.tar.zst"
        subprocess.run(["zstd", "-19", "-q", "-f", "-o", str(archive), str(tar_path)], check=True)
        return archive


def write_sums(directory: Path, out: Path) -> None:
    names = sorted(p.name for p in directory.iterdir() if p.is_file() and p.resolve() != out.resolve())
    out.write_text("".join(f"{sha256_of(directory / n)}  {n}\n" for n in names), newline="\n")


def build_index(directory: Path, tag: str, repo: str, date: str, set_id: str | None) -> dict:
    version = tag.removeprefix("v")
    base = f"https://github.com/{repo}/releases/download/{tag}"
    platforms: dict[str, dict] = {}
    for path in sorted(directory.iterdir()):
        match = ARCHIVE_RE.match(path.name)
        if not match:
            continue
        if match["v"] != version:
            raise SystemExit(f"{path.name} is not version {version}")
        if match["a"] != ARCH[match["p"]]:
            continue
        platforms[f"{match['a']}-{match['p']}"] = {
            "url": f"{base}/{path.name}",
            "sha256": sha256_of(path),
            "size": path.stat().st_size,
        }
    if not platforms:
        raise SystemExit(f"no release archive in {directory}")
    return {
        "schema_version": INDEX_SCHEMA_VERSION,
        "latest": version,
        "released": date,
        "notes_url": f"https://github.com/{repo}/releases/tag/{tag}",
        "platforms": platforms,
        "known_good_set": set_id,
        "advisories": [],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)
    n = sub.add_parser("notices")
    n.add_argument("--metadata", type=Path, required=True)
    n.add_argument("--out", type=Path, required=True)
    p = sub.add_parser("package")
    p.add_argument("--target", choices=sorted(ARCH), required=True)
    p.add_argument("--version", required=True)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--notices", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    s = sub.add_parser("sums")
    s.add_argument("--dir", type=Path, required=True)
    s.add_argument("--out", type=Path, required=True)
    i = sub.add_parser("index")
    i.add_argument("--dir", type=Path, required=True)
    i.add_argument("--tag", required=True)
    i.add_argument("--repo", required=True)
    i.add_argument("--date", required=True)
    i.add_argument("--set-id")
    i.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)

    if args.cmd == "notices":
        args.out.write_text(notices_text(json.loads(args.metadata.read_text())), newline="\n")
    elif args.cmd == "package":
        print(package(args.target, args.version, args.binary, args.notices, args.out))
    elif args.cmd == "sums":
        write_sums(args.dir, args.out)
    else:
        index = build_index(args.dir, args.tag, args.repo, args.date, args.set_id)
        args.out.write_text(json.dumps(index, indent=2) + "\n", newline="\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
