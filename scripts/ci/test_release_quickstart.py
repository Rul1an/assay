#!/usr/bin/env python3
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts/ci/release_readme.py"
RUNNER_PATH = ROOT / "examples/mcp-quickstart/run.py"
MOCK_PATH = ROOT / "examples/mcp-quickstart/mock_server.py"
CAPTURED_OUTPUT = """assay quickstart: PASS
mcp_requests=initialize,tools/list,tools/call
decision=allow tool=read_file
decision_artifact=.assay/quickstart/decisions.ndjson
non_claim=forwarded_to_local_mock_only
"""


def load_module():
    spec = importlib.util.spec_from_file_location("release_readme", MODULE_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load release README renderer")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_path(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def fenced_block_after(document: str, marker: str) -> str:
    tail = document.split(marker, 1)
    if len(tail) != 2:
        raise AssertionError(f"missing marker: {marker}")
    fence = tail[1].split("```text\n", 1)
    if len(fence) != 2:
        raise AssertionError(f"missing text fence after: {marker}")
    return fence[1].split("```", 1)[0]


class ReleaseReadmeTruth(unittest.TestCase):
    def test_renderer_moves_only_active_release_claims_to_assembled_version(self):
        module = load_module()
        source = """# Assay

```bash
cargo install assay-cli --version 5.4.0 --locked
```

Current release: [`v5.4.0`](https://github.com/Rul1an/assay/releases/tag/v5.4.0).

Historical note: v5.3.0 shipped earlier.
"""
        rendered = module.render_release_readme(source, "5.5.0")
        self.assertIn("cargo install assay-cli --version 5.5.0 --locked", rendered)
        self.assertIn(
            "Current release: [`v5.5.0`](https://github.com/Rul1an/assay/releases/tag/v5.5.0).",
            rendered,
        )
        self.assertIn("Historical note: v5.3.0 shipped earlier.", rendered)
        self.assertNotIn("cargo install assay-cli --version 5.4.0 --locked", rendered)

    def test_renderer_refuses_ambiguous_active_claims(self):
        module = load_module()
        source = """cargo install assay-cli --version 5.4.0 --locked
cargo install assay-cli --version 5.4.0 --locked
Current release: [`v5.4.0`](https://github.com/Rul1an/assay/releases/tag/v5.4.0).
"""
        with self.assertRaisesRegex(ValueError, "exactly one release-pinned install command"):
            module.render_release_readme(source, "5.5.0")

    def test_cli_accepts_the_release_contract_tag_and_writes_stdout(self):
        completed = subprocess.run(
            [sys.executable, str(MODULE_PATH), "v5.5.0"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("--version 5.5.0", completed.stdout)
        self.assertIn("releases/tag/v5.5.0", completed.stdout)

    def test_cli_refuses_a_bare_version_that_the_release_contract_never_emits(self):
        completed = subprocess.run(
            [sys.executable, str(MODULE_PATH), "5.5.0"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 2)

    def test_cli_preserves_the_release_contract_prerelease_suffix(self):
        completed = subprocess.run(
            [sys.executable, str(MODULE_PATH), "v5.5.0-rc.1"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("--version 5.5.0-rc.1", completed.stdout)
        self.assertIn("releases/tag/v5.5.0-rc.1", completed.stdout)

    def test_cli_refuses_caller_selected_source_and_output_paths(self):
        completed = subprocess.run(
            [sys.executable, str(MODULE_PATH), "README.md", "5.5.0", "out/README.md"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 2)


class QuickstartProcessBoundaries(unittest.TestCase):
    def test_runner_ignores_assay_bin_and_invokes_the_fixed_command_name(self):
        runner = load_path("mcp_quickstart_run", RUNNER_PATH)
        with mock.patch.dict(os.environ, {"ASSAY_BIN": "/tmp/not-assay"}), mock.patch.object(
            runner.shutil, "which", return_value="/trusted/assay"
        ):
            self.assertEqual(runner.resolve_assay(), "assay")

    def test_mock_records_only_under_its_working_directory(self):
        request = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {},
        }
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / ".assay/quickstart").mkdir(parents=True)
            outside = root.parent / f"{root.name}-outside.json"
            env = os.environ.copy()
            env["ASSAY_QUICKSTART_INVOCATION_LOG"] = str(outside)
            completed = subprocess.run(
                [sys.executable, str(MOCK_PATH)],
                cwd=root,
                env=env,
                input=json.dumps(request) + "\n",
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertFalse(outside.exists())
            self.assertTrue((root / ".assay/quickstart/mock-invocation.json").is_file())


class ReleaseArchiveShape(unittest.TestCase):
    def test_all_cli_archives_carry_the_bounded_quickstart(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
        required_twice = [
            'scripts/ci/release_readme.py "$VERSION"',
            "examples/mcp-quickstart/policy.yaml",
            "examples/mcp-quickstart/run.py",
            "examples/mcp-quickstart/mock_server.py",
        ]
        for fragment in required_twice:
            with self.subTest(fragment=fragment):
                self.assertEqual(workflow.count(fragment), 2)

    def test_readmes_label_the_runner_output_as_captured(self):
        root_readme = (ROOT / "README.md").read_text(encoding="utf-8")
        example_readme = (ROOT / "examples/mcp-quickstart/README.md").read_text(encoding="utf-8")
        self.assertIn("Captured runner output", root_readme)
        self.assertIn("Captured runner output", example_readme)
        self.assertEqual(
            fenced_block_after(root_readme, "Captured runner output"), CAPTURED_OUTPUT
        )
        self.assertEqual(
            fenced_block_after(example_readme, "Captured runner output"), CAPTURED_OUTPUT
        )


if __name__ == "__main__":
    unittest.main()
