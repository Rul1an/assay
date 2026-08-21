#!/usr/bin/env python3
"""Exercise the adequacy lane's Cargo cleanup after Rust is installed."""

from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


class CleanupBoundary(unittest.TestCase):
    def test_repository_config_cannot_redirect_cleanup(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            parent = Path(td)
            workspace = parent / "workspace"
            victim = parent / "victim"
            (workspace / ".cargo").mkdir(parents=True)
            (workspace / "target").mkdir()
            victim.mkdir()
            (workspace / "Cargo.toml").write_text(
                '[package]\nname = "cleanup-fixture"\nversion = "0.0.0"\n',
                encoding="utf-8",
            )
            (workspace / "src").mkdir()
            (workspace / "src/lib.rs").write_text("", encoding="utf-8")
            (workspace / ".cargo/config.toml").write_text(
                '[build]\ntarget-dir = "../victim"\n', encoding="utf-8"
            )
            (workspace / "target/generated").write_text("generated", encoding="utf-8")
            (victim / "sentinel").write_text("not cargo output", encoding="utf-8")

            env = os.environ.copy()
            env["GITHUB_WORKSPACE"] = str(workspace)
            subprocess.run(
                ["cargo", "clean", "--target-dir", str(workspace / "target")],
                cwd=workspace,
                env=env,
                check=True,
                capture_output=True,
                text=True,
            )

            self.assertTrue((victim / "sentinel").is_file())
            self.assertFalse((workspace / "target").exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
