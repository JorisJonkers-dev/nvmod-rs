#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Repository guards: SPDX headers, no game data or binaries, .gitignore.

    repo_guards.py [--root DIR] {spdx,data,gitignore,all}

Every tracked file is checked. Exit status 0 when clean, 1 when a rule is broken (one line per violation).

  spdx       every .rs, .py and .sh file has `SPDX-License-Identifier: GPL-3.0-only` in its first five lines.
  data       no file with a game-data, mod-archive or Windows-binary extension, no file over MAX_BYTES and no file
             with NUL bytes. Test files are built in code (crates/nvmod-testkit); there is no allow-list.
  gitignore  .gitignore lists every one of those extensions.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

SOURCE_EXTS = {".rs", ".py", ".sh"}
SPDX_RE = re.compile(r"SPDX-License-Identifier:\s*GPL-3\.0-only(?![\w.-])")
SPDX_LINES = 5
FORBIDDEN_EXTS = (
    ".esm", ".esp", ".esl", ".bsa", ".ba2", ".nif", ".kf", ".dds", ".btr", ".bto", ".btt", ".lod",
    ".wav", ".ogg", ".mp3", ".fos", ".ess", ".dll", ".exe", ".asi", ".7z", ".rar", ".zip",
)  # fmt: skip
MAX_BYTES = 256 * 1024


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


def run(root: Path, command: str) -> list[str]:
    files = tracked_files(root)
    bad: list[str] = []
    if command in ("spdx", "all"):
        bad += check_spdx(root, files)
    if command in ("data", "all"):
        bad += check_data(root, files)
    if command in ("gitignore", "all"):
        bad += check_gitignore(root)
    return bad


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["spdx", "data", "gitignore", "all"])
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
