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

CONFIG = (
    '{"packages": {".": {"extra-files": [{"type": "toml", "path": "Cargo.toml", '
    '"jsonpath": "$.workspace.package.version"}]}}}'
)


def workspace(members: list[str], **overrides: str) -> dict[str, str]:
    listed = ", ".join(f'"crates/{m}"' for m in members)
    files = {
        "Cargo.toml": f'[workspace]\nmembers = [{listed}]\nexclude = ["fuzz"]\n[workspace.package]\nversion = "1.0.0"\n',
        "release-please-config.json": CONFIG,
    }
    for m in members:
        files[f"crates/{m}/Cargo.toml"] = f'[package]\nname = "{m}"\nversion.workspace = true\n'
    files.update(overrides)
    return files


class Guards(unittest.TestCase):
    def test_a_clean_repository_passes(self):
        root = make_repo({**workspace(["a"]), ".gitignore": IGNORE, "a.rs": GOOD_RS})
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

    def test_a_workspace_covered_by_release_handling_passes(self):
        self.assertEqual(repo_guards.run(make_repo(workspace(["a", "b"])), "release"), [])

    def test_a_crate_with_its_own_version_is_reported(self):
        files = workspace(["a", "b"], **{"crates/b/Cargo.toml": '[package]\nname = "b"\nversion = "0.1.0"\n'})
        bad = repo_guards.run(make_repo(files), "release")
        self.assertEqual(len(bad), 1)
        self.assertIn("crates/b/Cargo.toml", bad[0])

    def test_a_new_member_without_workspace_version_is_reported(self):
        files = workspace(["a"])
        files["Cargo.toml"] = files["Cargo.toml"].replace('"crates/a"', '"crates/a", "crates/new"')
        files["crates/new/Cargo.toml"] = '[package]\nname = "new"\n'
        self.assertEqual(len(repo_guards.run(make_repo(files), "release")), 1)

    def test_a_missing_workspace_version_entry_is_reported(self):
        files = workspace(["a"], **{"release-please-config.json": '{"packages": {".": {"extra-files": []}}}'})
        bad = repo_guards.run(make_repo(files), "release")
        self.assertEqual(len(bad), 1)
        self.assertIn("workspace.package.version", bad[0])

    def test_an_entry_for_a_crate_that_is_not_a_member_is_reported(self):
        config = CONFIG.replace("]}}}", ', {"type": "toml", "path": "crates/gone/Cargo.toml", "jsonpath": "$.package.version"}]}}}')
        bad = repo_guards.run(make_repo(workspace(["a"], **{"release-please-config.json": config})), "release")
        self.assertEqual(len(bad), 1)
        self.assertIn("crates/gone/Cargo.toml", bad[0])

    def test_main_returns_the_exit_status(self):
        good = make_repo({**workspace(["a"]), ".gitignore": IGNORE, "a.rs": GOOD_RS})
        bad = make_repo({**workspace(["a"]), ".gitignore": IGNORE, "a.rs": "fn main() {}\n"})
        self.assertEqual(repo_guards.main(["--root", str(good), "all"]), 0)
        self.assertEqual(repo_guards.main(["--root", str(bad), "all"]), 1)


if __name__ == "__main__":
    unittest.main()
