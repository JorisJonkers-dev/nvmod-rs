# SPDX-License-Identifier: GPL-3.0-only
"""Self-tests of package_release.py: no network, no cargo."""

import io
import json
import subprocess
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

import package_release as pr

METADATA = {
    "packages": [
        {"id": "cli", "name": "nvmod-cli", "version": "1.0.0"},
        {"id": "a", "name": "serde", "version": "1.0.0", "license": "MIT OR Apache-2.0", "repository": "https://x/serde"},
        {"id": "d", "name": "tempfile", "version": "3.0.0", "license": "MIT"},
        {"id": "b", "name": "nvmod-core", "version": "1.0.0"},
    ],
    "resolve": {
        "nodes": [
            {"id": "cli", "deps": [
                {"pkg": "a", "dep_kinds": [{"kind": None}]},
                {"pkg": "b", "dep_kinds": [{"kind": None}]},
                {"pkg": "d", "dep_kinds": [{"kind": "dev"}]},
            ]},
            {"id": "a", "deps": []},
            {"id": "b", "deps": []},
            {"id": "d", "deps": []},
        ]
    },
}


class PackageReleaseTests(unittest.TestCase):
    def test_notices_list_linked_third_party_crates_only(self):
        text = pr.notices_text(METADATA)
        self.assertIn("serde 1.0.0", text)
        self.assertIn("MIT OR Apache-2.0", text)
        self.assertNotIn("tempfile", text)
        self.assertNotIn("nvmod-core", text)

    def test_windows_zip_is_portable(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "bin").write_bytes(b"binary")
            (tmp / "notices").write_text("n")
            archive = pr.package("windows", "1.2.3", tmp / "bin", tmp / "notices", tmp / "out")
            self.assertEqual(archive.name, "nvmod-1.2.3-x86_64-windows.zip")
            names = zipfile.ZipFile(archive).namelist()
            self.assertIn("nvmod-1.2.3-x86_64-windows/nvmod.exe", names)
            self.assertIn("nvmod-1.2.3-x86_64-windows/nvmod.portable", names)
            self.assertIn("nvmod-1.2.3-x86_64-windows/LICENSE", names)

    def test_macos_tarball_is_universal(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "bin").write_bytes(b"binary")
            (tmp / "notices").write_text("n")
            archive = pr.package("macos", "1.2.3", tmp / "bin", tmp / "notices", tmp / "out")
            self.assertEqual(archive.name, "nvmod-1.2.3-universal-macos.tar.zst")
            listing = subprocess.run(["zstd", "-dc", str(archive)], check=True, capture_output=True).stdout
            with tarfile.open(fileobj=io.BytesIO(listing)) as tf:
                names = tf.getnames()
                self.assertEqual(tf.getmember("nvmod-1.2.3-universal-macos/nvmod").mode, 0o755)
            self.assertIn("nvmod-1.2.3-universal-macos/LICENSE", names)
            self.assertNotIn("nvmod-1.2.3-universal-macos/nvmod.portable", names)

    def test_sums_and_index(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "nvmod-1.2.3-x86_64-linux.tar.zst").write_bytes(b"abc")
            (tmp / "nvmod-1.2.3-x86_64-windows.zip").write_bytes(b"abcd")
            (tmp / "nvmod-1.2.3-universal-macos.tar.zst").write_bytes(b"abcde")
            sums = tmp / "SHA256SUMS"
            pr.write_sums(tmp, sums)
            self.assertIn(
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  nvmod-1.2.3-x86_64-linux.tar.zst",
                sums.read_text(),
            )
            index = pr.build_index(tmp, "v1.2.3", "o/r", "2026-11-14", None)
            self.assertEqual(index["latest"], "1.2.3")
            self.assertEqual(index["platforms"]["x86_64-linux"]["size"], 3)
            self.assertEqual(
                index["platforms"]["x86_64-windows"]["url"],
                "https://github.com/o/r/releases/download/v1.2.3/nvmod-1.2.3-x86_64-windows.zip",
            )
            self.assertEqual(index["platforms"]["universal-macos"]["size"], 5)
            self.assertEqual(index["advisories"], [])
            json.dumps(index)

    def test_index_refuses_a_version_mismatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "nvmod-9.9.9-x86_64-linux.tar.zst").write_bytes(b"x")
            with self.assertRaises(SystemExit):
                pr.build_index(Path(tmp), "v1.2.3", "o/r", "2026-11-14", None)


if __name__ == "__main__":
    unittest.main()
