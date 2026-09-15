#!/usr/bin/env python3
"""Exercise release-version handling through the real wheel smoke entry point."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location(
    "wheel_smoke", Path(__file__).with_name("smoke-python-wheel.py")
)
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)


class ReleaseVersions(unittest.TestCase):
    def exercise(self, rust, python, *, filename_version=None):
        with tempfile.TemporaryDirectory() as scratch, patch.dict(os.environ):
            root = Path(scratch)
            (root / "Cargo.toml").write_text(f'[workspace.package]\nversion = "{rust}"\n')
            matrix = root / smoke.MATRIX_REL
            matrix.parent.mkdir()
            matrix.write_text(json.dumps({"package": "assay-it", "wheels": [{
                "target": "aarch64-apple-darwin", "import_smoke": "native",
                "tag": "cp312-cp312-macosx_11_0_arm64",
            }]}))
            dist = root / "dist"
            dist.mkdir()
            wheel = dist / f'assay_it-{filename_version or python}-cp312-cp312-macosx_11_0_arm64.whl'
            with zipfile.ZipFile(wheel, "w") as archive:
                archive.writestr("assay/_native.so", b"fixture, not a live native extension")
            with patch.object(smoke.shutil, "which", return_value="/python"), patch.object(
                smoke.subprocess, "run"
            ) as run:
                self.assertEqual(smoke.main([
                    "--root", str(root), "--dist-dir", "dist",
                    "--target", "aarch64-apple-darwin", "--python", "python3",
                    "--package", "assay-it",
                ]), 0)
            self.assertEqual(run.call_count, 3)
            install = run.call_args_list[1].args[0]
            self.assertEqual(install[-1], f"assay-it=={python}")
            probe = run.call_args_list[2].args[0][-1]
            self.assertIn(f"== {python!r}", probe)
            self.assertIn("assay._native", probe)

    def test_stable(self):
        self.exercise("6.3.1", "6.3.1")

    def test_rc(self):
        self.exercise("6.3.1-rc.1", "6.3.1rc1")

    def test_beta(self):
        self.exercise("6.3.1-beta.12", "6.3.1b12")

    def test_wrong_candidate_is_refused(self):
        with self.assertRaises(SystemExit):
            self.exercise("6.3.1-rc.2", "6.3.1rc2", filename_version="6.3.1rc1")

    def test_unknown_channel_is_refused(self):
        with self.assertRaises(SystemExit):
            self.exercise("6.3.1-sentinel", "6.3.1-sentinel")


if __name__ == "__main__":
    unittest.main()
