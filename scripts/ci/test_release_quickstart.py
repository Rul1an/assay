#!/usr/bin/env python3
import importlib.util
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts/ci/release_readme.py"
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

    def test_cli_writes_the_rendered_readme(self):
        module = load_module()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "README.md"
            output = root / "dist/README.md"
            source.write_text(
                "cargo install assay-cli --version 5.4.0 --locked\n"
                "Current release: [`v5.4.0`](https://github.com/Rul1an/assay/releases/tag/v5.4.0).\n",
                encoding="utf-8",
            )
            self.assertEqual(module.main([str(source), "5.5.0", str(output)]), 0)
            self.assertIn("--version 5.5.0", output.read_text(encoding="utf-8"))


class ReleaseArchiveShape(unittest.TestCase):
    def test_all_cli_archives_carry_the_bounded_quickstart(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
        required_twice = [
            "scripts/ci/release_readme.py README.md",
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
