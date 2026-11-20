#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Repository guards: SPDX headers, no game data or binaries, .gitignore.

    repo_guards.py [--root DIR] {spdx,data,gitignore,release,decisions,all}

Every tracked file is checked. Exit status 0 when clean, 1 when a rule is broken (one line per violation).

  spdx       every .rs, .py and .sh file has `SPDX-License-Identifier: GPL-3.0-only` in its first five lines.
  data       no file with a game-data, mod-archive or Windows-binary extension, no file over MAX_BYTES and no file
             with NUL bytes. Test files are built in code (crates/nvmod-testkit); there is no allow-list.
  gitignore  .gitignore lists every one of those extensions.
  release    every workspace crate is covered by release handling: its Cargo.toml inherits
             `version.workspace = true` (so the one `workspace.package.version` that release-please bumps is its
             version, and release.yml's `cargo update --workspace` then refreshes its Cargo.lock entry),
             release-please-config.json bumps that workspace version, and no extra-files entry names a manifest that
             is not a workspace member. `fuzz` is its own workspace and is excluded.
  decisions  docs/DECISIONS.md is frozen history and never grows: a decision is one new file under docs/decisions/
             (see docs/decisions/README.md), so parallel pull requests do not conflict on one table.
"""

from __future__ import annotations

import argparse
import glob
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

SOURCE_EXTS = {".rs", ".py", ".sh"}
SPDX_RE = re.compile(r"SPDX-License-Identifier:\s*GPL-3\.0-only(?![\w.-])")
SPDX_LINES = 5
FORBIDDEN_EXTS = (
    ".esm", ".esp", ".esl", ".bsa", ".ba2", ".nif", ".kf", ".dds", ".btr", ".bto", ".btt", ".lod",
    ".wav", ".ogg", ".mp3", ".fos", ".ess", ".dll", ".exe", ".asi", ".7z", ".rar", ".zip",
)  # fmt: skip
MAX_BYTES = 256 * 1024
RELEASE_CONFIG = "release-please-config.json"
WORKSPACE_VERSION_PATH = "$.workspace.package.version"
EXCLUDED_CRATES = {"fuzz"}
DECISIONS_FILE = "docs/DECISIONS.md"
DECISIONS_FROZEN_LINES = 315


def tracked_files(root: Path) -> list[str]:
    result = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z"], capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        raise SystemExit(f"git ls-files failed: {result.stderr.strip()}")
    return [p for p in result.stdout.split("\0") if p]


def check_spdx(root: Path, files: list[str]) -> list[str]:
    bad = []
    for rel in files:
        if Path(rel).suffix not in SOURCE_EXTS:
            continue
        try:
            text = (root / rel).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            bad.append(f"{rel}: cannot be read as UTF-8 text")
            continue
        if not SPDX_RE.search("\n".join(text.splitlines()[:SPDX_LINES])):
            bad.append(f"{rel}: no `SPDX-License-Identifier: GPL-3.0-only` in the first {SPDX_LINES} lines")
    return bad


def check_data(root: Path, files: list[str]) -> list[str]:
    bad = []
    for rel in files:
        path = root / rel
        if not path.is_file():
            continue
        suffix = Path(rel).suffix.lower()
        if suffix in FORBIDDEN_EXTS:
            bad.append(f"{rel}: extension {suffix}: game data, mods and binaries never enter the repository")
        elif path.stat().st_size > MAX_BYTES:
            bad.append(f"{rel}: over the {MAX_BYTES} byte limit for a committed file")
        elif b"\0" in path.read_bytes():
            bad.append(f"{rel}: binary content (NUL byte): build fixtures in code instead")
    return bad


def check_gitignore(root: Path) -> list[str]:
    try:
        text = (root / ".gitignore").read_text(encoding="utf-8")
    except OSError:
        return [".gitignore: missing"]
    entries = {line.strip() for line in text.splitlines()}
    return [f".gitignore: does not ignore *{ext}" for ext in FORBIDDEN_EXTS if f"*{ext}" not in entries]


def workspace_members(root: Path) -> tuple[list[str], list[str]]:
    """The workspace member directories (globs expanded, `fuzz` left out) and any problems reading them."""
    try:
        manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as e:
        return [], [f"Cargo.toml: cannot be read: {e}"]
    members: list[str] = []
    for pattern in manifest.get("workspace", {}).get("members", []):
        found = sorted(
            Path(m).relative_to(root).as_posix() for m in glob.glob(str(root / pattern)) if Path(m).is_dir()
        )
        members += found or [pattern]
    return [m for m in members if Path(m).name not in EXCLUDED_CRATES], []


def check_release(root: Path) -> list[str]:
    members, bad = workspace_members(root)
    if bad:
        return bad
    try:
        config = json.loads((root / RELEASE_CONFIG).read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as e:
        return [f"{RELEASE_CONFIG}: cannot be read: {e}"]
    extra = [
        entry
        for package in config.get("packages", {}).values()
        for entry in package.get("extra-files", [])
        if isinstance(entry, dict)
    ]
    if not any(e.get("path") == "Cargo.toml" and e.get("jsonpath") == WORKSPACE_VERSION_PATH for e in extra):
        bad.append(f"{RELEASE_CONFIG}: no extra-files entry bumps {WORKSPACE_VERSION_PATH} of Cargo.toml")
    manifests = {f"{m}/Cargo.toml" for m in members}
    for entry in extra:
        path = str(entry.get("path", ""))
        if path.endswith("/Cargo.toml") and path not in manifests:
            bad.append(f"{RELEASE_CONFIG}: extra-files names {path}, which is not a workspace member")
    for member in members:
        try:
            package = tomllib.loads((root / member / "Cargo.toml").read_text(encoding="utf-8")).get("package", {})
        except (OSError, tomllib.TOMLDecodeError):
            bad.append(f"{member}/Cargo.toml: workspace member without a readable manifest")
            continue
        if package.get("version") != {"workspace": True}:
            bad.append(
                f"{member}/Cargo.toml: must use `version.workspace = true`, so release-please's bump of the "
                "workspace version reaches this crate and its Cargo.lock entry"
            )
    return bad


def check_decisions(root: Path) -> list[str]:
    path = root / DECISIONS_FILE
    if not path.is_file():
        return []
    lines = len(path.read_text(encoding="utf-8").splitlines())
    if lines > DECISIONS_FROZEN_LINES:
        return [
            f"{DECISIONS_FILE}: {lines} lines, frozen at {DECISIONS_FROZEN_LINES}; record a decision as one new file "
            "docs/decisions/<YYYY-MM-DD>-<slug>.md instead (docs/decisions/README.md)"
        ]
    return []


def run(root: Path, command: str) -> list[str]:
    files = tracked_files(root)
    bad: list[str] = []
    if command in ("spdx", "all"):
        bad += check_spdx(root, files)
    if command in ("data", "all"):
        bad += check_data(root, files)
    if command in ("gitignore", "all"):
        bad += check_gitignore(root)
    if command in ("release", "all"):
        bad += check_release(root)
    if command in ("decisions", "all"):
        bad += check_decisions(root)
    return bad


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["spdx", "data", "gitignore", "release", "decisions", "all"])
    parser.add_argument("--root", type=Path, default=Path("."))
    args = parser.parse_args(argv)
    violations = run(args.root.resolve(), args.command)
    for line in violations:
        print(line)
    if violations:
        print(f"repository guards: {len(violations)} violation(s)", file=sys.stderr)
        return 1
    print(f"repository guards ({args.command}): ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
