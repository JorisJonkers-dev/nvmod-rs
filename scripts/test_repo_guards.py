# SPDX-License-Identifier: GPL-3.0-only
"""Self-tests of repo_guards.py, on throwaway git repositories."""

import subprocess
import tempfile
import unittest
from pathlib import Path

import repo_guards

IGNORE = "\n".join(f"*{e}" for e in repo_guards.FORBIDDEN_EXTS) + "\n"
GOOD_RS = "// SPDX-License-Identifier: GPL-3.0-only\nfn main() {}\n"


def make_repo(files: dict[str, bytes | str]) -> Path:
    root = Path(tempfile.mkdtemp())
    subprocess.run(["git", "init", "-q", str(root)], check=True)
    for rel, content in files.items():
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content.encode() if isinstance(content, str) else content)
    subprocess.run(["git", "-C", str(root), "add", "-f", "."], check=True)
    return root


class Guards(unittest.TestCase):
    def test_a_clean_repository_passes(self):
        root = make_repo({".gitignore": IGNORE, "a.rs": GOOD_RS})
        self.assertEqual(repo_guards.run(root, "all"), [])

    def test_a_source_file_without_the_licence_header_is_reported(self):
        root = make_repo({".gitignore": IGNORE, "a.rs": "fn main() {}\n", "b.py": "# SPDX-License-Identifier: MPL-2.0\n"})
        bad = repo_guards.run(root, "spdx")
        self.assertEqual(len(bad), 2)
        self.assertIn("a.rs", bad[0])

    def test_game_data_and_binaries_are_reported_by_extension_case_insensitively(self):
        root = make_repo({".gitignore": IGNORE, "x/plugin.DLL": "MZ", "m.esp": "TES4", "readme.md": "ok"})
        bad = repo_guards.run(root, "data")
        self.assertEqual(sorted(b.split(":")[0] for b in bad), ["m.esp", "x/plugin.DLL"])

    def test_nul_bytes_and_large_files_are_reported(self):
        root = make_repo({"a.bin": b"a\0b", "big.txt": "x" * (repo_guards.MAX_BYTES + 1)})
        self.assertEqual(len(repo_guards.run(root, "data")), 2)

    def test_gitignore_must_list_every_extension(self):
        self.assertEqual(len(repo_guards.run(make_repo({"a.txt": "x"}), "gitignore")), 1)
        root = make_repo({".gitignore": "*.dll\n"})
        self.assertEqual(len(repo_guards.run(root, "gitignore")), len(repo_guards.FORBIDDEN_EXTS) - 1)

    def test_main_returns_the_exit_status(self):
        good = make_repo({".gitignore": IGNORE, "a.rs": GOOD_RS})
        bad = make_repo({".gitignore": IGNORE, "a.rs": "fn main() {}\n"})
        self.assertEqual(repo_guards.main(["--root", str(good), "all"]), 0)
        self.assertEqual(repo_guards.main(["--root", str(bad), "all"]), 1)


if __name__ == "__main__":
    unittest.main()
