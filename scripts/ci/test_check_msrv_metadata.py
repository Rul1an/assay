#!/usr/bin/env python3

import json
import os
import subprocess
import sys
import unittest
from pathlib import Path

from check_msrv_metadata import select_public_workspace_packages


class PublicWorkspacePackageSelectionTests(unittest.TestCase):
    repo_root = Path(__file__).resolve().parents[2]

    @staticmethod
    def metadata_fixture() -> dict[str, object]:
        return {
            "workspace_members": [
                "workspace-public 1.0.0 (path+file:///repo/public)",
                "workspace-private 1.0.0 (path+file:///repo/private)",
            ],
            "packages": [
                {
                    "id": "workspace-public 1.0.0 (path+file:///repo/public)",
                    "name": "workspace-public",
                    "source": None,
                    "publish": None,
                    "rust_version": "1.89",
                },
                {
                    "id": "workspace-private 1.0.0 (path+file:///repo/private)",
                    "name": "workspace-private",
                    "source": None,
                    "publish": [],
                    "rust_version": None,
                },
                {
                    "id": "external-path 1.0.0 (path+file:///tmp/external)",
                    "name": "external-path",
                    "source": None,
                    "publish": None,
                    "rust_version": "1.99",
                },
            ],
        }

    def test_excludes_publishable_path_dependency_outside_workspace(self) -> None:
        selected = select_public_workspace_packages(self.metadata_fixture())

        self.assertEqual(
            [package["name"] for package in selected],
            ["workspace-public"],
        )

    def test_cli_reads_metadata_from_stdin_and_writes_package_names_to_stdout(
        self,
    ) -> None:
        result = subprocess.run(
            [
                sys.executable,
                str(Path(__file__).with_name("check_msrv_metadata.py")),
                "1.89",
            ],
            input=json.dumps(self.metadata_fixture()),
            capture_output=True,
            check=False,
            text=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "workspace-public\n")
        self.assertIn("public-msrv-metadata=1.89", result.stderr)
        self.assertIn("public-crates=1", result.stderr)

    def test_policy_check_requires_explicit_msrv_toolchain(self) -> None:
        env = os.environ.copy()
        env.pop("ASSAY_PUBLIC_MSRV", None)
        env["ASSAY_MSRV_METADATA_ONLY"] = "1"

        result = subprocess.run(
            ["scripts/ci/check-msrv-policy.sh"],
            cwd=self.repo_root,
            env=env,
            capture_output=True,
            check=False,
            text=True,
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ASSAY_PUBLIC_MSRV must be set", result.stderr)


if __name__ == "__main__":
    unittest.main()
