#!/usr/bin/env python3
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import time
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

For v5.4.0, run the last command from a source checkout. The installer and the
published v5.4.0 CLI archive install the binary but do not carry the bounded
quickstart assets.

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

    def test_readmes_do_not_claim_the_quickstart_is_in_cli_archives(self):
        root_readme = (ROOT / "README.md").read_text(encoding="utf-8")
        example_readme = (ROOT / "examples/mcp-quickstart/README.md").read_text(
            encoding="utf-8"
        )

        self.assertIn(
            "For v5.4.0, run the last command from a source checkout.",
            root_readme,
        )
        for readme in (root_readme, example_readme):
            self.assertNotIn("root of an extracted CLI release archive", readme)
            self.assertNotIn("archive carries this bounded quickstart", readme)




PACKED_SOURCE_MEMBERS = (
    ("binary", "${{ matrix.artifact }}"),
    ("license", "LICENSE"),
    ("quickstart-policy", "examples/mcp-quickstart/policy.yaml"),
    ("quickstart-run", "examples/mcp-quickstart/run.py"),
    ("quickstart-mock", "examples/mcp-quickstart/mock_server.py"),
)
CHECKOUT_SENTENCE = "For v5.4.0, run the last command from a source checkout."
ARCHIVE_ROOT_CLAIM = "From the root of this extracted CLI archive"
QUICKSTART_COMMAND = "python3 examples/mcp-quickstart/run.py"


def package_step(workflow: str, heading: str) -> str:
    marker = f"      - name: {heading}\n"
    parts = workflow.split(marker)
    if len(parts) != 2:
        raise AssertionError(f"expected one {heading!r} step")
    rest = parts[1]
    nxt = rest.find("\n      - name: ")
    if nxt == -1:
        raise AssertionError(f"{heading!r} step was not followed by another named step")
    return rest[:nxt]


class ReleaseArchiveMemberInventory(unittest.TestCase):
    def test_unix_and_windows_package_steps_copy_five_packed_source_classes(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
        unix = package_step(workflow, "Package (Unix)")
        windows = package_step(workflow, "Package (Windows)")
        self.assertIn('scripts/ci/release_readme.py "$VERSION"', unix)
        self.assertIn('scripts/ci/release_readme.py "$VERSION"', windows)
        for name, fragment in PACKED_SOURCE_MEMBERS:
            with self.subTest(platform="unix", member=name):
                self.assertIn(fragment, unix)
            with self.subTest(platform="windows", member=name):
                self.assertIn(fragment, windows)


class ReleaseArchiveReadmeContract(unittest.TestCase):
    def test_source_readme_keeps_published_v54_checkout_wording(self):
        source = (ROOT / "README.md").read_text(encoding="utf-8")
        self.assertIn(CHECKOUT_SENTENCE, source)
        self.assertNotIn(ARCHIVE_ROOT_CLAIM, source)

    def test_rendered_readme_rewrites_absent_links_and_states_packed_quickstart(self):
        module = load_module()
        source = (ROOT / "README.md").read_text(encoding="utf-8")
        rendered = module.render_release_readme(source, "5.5.0")
        self.assertNotIn(CHECKOUT_SENTENCE, rendered)
        self.assertIn(ARCHIVE_ROOT_CLAIM, rendered)
        self.assertIn(QUICKSTART_COMMAND, rendered)
        self.assertIn("examples/mcp-quickstart/policy.yaml", rendered)
        self.assertIn("examples/mcp-quickstart/run.py", rendered)
        self.assertIn("examples/mcp-quickstart/mock_server.py", rendered)
        self.assertIn("assay` on PATH", rendered)
        self.assertIn("[MIT](LICENSE)", rendered)
        self.assertIn("[MCP Quick Start](examples/mcp-quickstart/)", rendered)
        self.assertIn("[MCP Quickstart](examples/mcp-quickstart/)", rendered)
        self.assertIn('href="examples/mcp-quickstart/"', rendered)
        self.assertIn(
            "[CHANGELOG.md](https://github.com/Rul1an/assay/blob/v5.5.0/CHANGELOG.md)",
            rendered,
        )
        self.assertIn(
            "[release-pinned agent journey](https://github.com/Rul1an/assay/blob/v5.5.0/docs/guides/agent-golden-path.md)",
            rendered,
        )
        self.assertIn(
            "(https://github.com/Rul1an/assay/tree/v5.5.0/examples/privileged-action-gate/)",
            rendered,
        )
        self.assertIn(
            "(https://github.com/Rul1an/assay/blob/v5.5.0/demo/output/screenshots/mcp-wrap-demo.svg)",
            rendered,
        )
        self.assertIn(
            'href="https://github.com/Rul1an/assay/blob/v5.5.0/docs/security/OWASP-MCP-TOP10-MAPPING.md"',
            rendered,
        )
        self.assertNotIn("(docs/guides/agent-golden-path.md)", rendered)
        self.assertNotIn("(CHANGELOG.md)", rendered)
        self.assertNotIn("[MIT](https://github.com/Rul1an/assay/blob/v5.5.0/LICENSE)", rendered)

    def test_renderer_keeps_https_and_hash_targets(self):
        module = load_module()
        source = """# Assay

```bash
cargo install assay-cli --version 5.4.0 --locked
```

Current release: [`v5.4.0`](https://github.com/Rul1an/assay/releases/tag/v5.4.0).

For v5.4.0, run the last command from a source checkout. The installer and the
published v5.4.0 CLI archive install the binary but do not carry the bounded
quickstart assets.

See [scope](docs/concepts/scope.md) and [license](LICENSE) and [quickstart](examples/mcp-quickstart/run.py) and [here](#quickstart) and [action](https://github.com/marketplace/actions/assay-ai-agent-security).
"""
        rendered = module.render_release_readme(source, "5.5.0")
        self.assertIn("[scope](https://github.com/Rul1an/assay/blob/v5.5.0/docs/concepts/scope.md)", rendered)
        self.assertIn("[license](LICENSE)", rendered)
        self.assertIn("[quickstart](examples/mcp-quickstart/run.py)", rendered)
        self.assertIn("[here](#quickstart)", rendered)
        self.assertIn("[action](https://github.com/marketplace/actions/assay-ai-agent-security)", rendered)
        self.assertIn(ARCHIVE_ROOT_CLAIM, rendered)
        self.assertNotIn(CHECKOUT_SENTENCE, rendered)

    def test_renderer_refuses_a_source_with_zero_checkout_matches(self):
        module = load_module()
        source = """cargo install assay-cli --version 5.4.0 --locked
Current release: [`v5.4.0`](https://github.com/Rul1an/assay/releases/tag/v5.4.0).
"""
        with self.assertRaisesRegex(ValueError, "exactly one published-checkout sentence"):
            rendered = module.render_release_readme(source, "5.5.0")
            returned_without_archive_truth = ARCHIVE_ROOT_CLAIM not in rendered
            self.fail(f"returned_without_archive_truth={returned_without_archive_truth}")

    def test_renderer_refuses_ambiguous_checkout_sentence(self):
        module = load_module()
        source = """cargo install assay-cli --version 5.4.0 --locked
Current release: [`v5.4.0`](https://github.com/Rul1an/assay/releases/tag/v5.4.0).
For v5.4.0, run the last command from a source checkout. The installer and the
published v5.4.0 CLI archive install the binary but do not carry the bounded
quickstart assets.
For v5.4.0, run the last command from a source checkout. The installer and the
published v5.4.0 CLI archive install the binary but do not carry the bounded
quickstart assets.
"""
        with self.assertRaisesRegex(ValueError, "exactly one published-checkout sentence"):
            module.render_release_readme(source, "5.5.0")

    def test_renderer_rejects_the_polynomial_markdown_link_regex(self):
        source = MODULE_PATH.read_text(encoding="utf-8")
        self.assertNotIn("MARKDOWN_LINK_RE", source)
        self.assertNotIn(r"(!?\[[^\]]*\]\()", source)

    def test_renderer_rewrites_hostile_bracket_input_in_bounded_time(self):
        module = load_module()
        hostile = "[" * 20_000 + "[](" * 20_000
        source = (
            "cargo install assay-cli --version 5.4.0 --locked\n"
            "Current release: [`v5.4.0`](https://github.com/Rul1an/assay/releases/tag/v5.4.0).\n"
            "For v5.4.0, run the last command from a source checkout. The installer and the\n"
            "published v5.4.0 CLI archive install the binary but do not carry the bounded\n"
            "quickstart assets.\n"
            + hostile
            + "\nSee [scope](docs/concepts/scope.md) and [license](LICENSE) and "
            "[quickstart](examples/mcp-quickstart/run.py) and [here](#quickstart) and "
            "[action](https://github.com/marketplace/actions/assay-ai-agent-security).\n"
            '<a href="docs/concepts/scope.md">scope</a>\n'
        )
        started = time.perf_counter()
        rendered = module.render_release_readme(source, "5.5.0")
        elapsed = time.perf_counter() - started
        self.assertLess(elapsed, 0.25, f"rewrite exceeded linear ceiling: {elapsed:.3f}s")
        self.assertIn(
            "[scope](https://github.com/Rul1an/assay/blob/v5.5.0/docs/concepts/scope.md)",
            rendered,
        )
        self.assertIn("[license](LICENSE)", rendered)
        self.assertIn("[quickstart](examples/mcp-quickstart/run.py)", rendered)
        self.assertIn("[here](#quickstart)", rendered)
        self.assertIn(
            "[action](https://github.com/marketplace/actions/assay-ai-agent-security)",
            rendered,
        )
        self.assertIn(
            'href="https://github.com/Rul1an/assay/blob/v5.5.0/docs/concepts/scope.md"',
            rendered,
        )
        self.assertNotIn(CHECKOUT_SENTENCE, rendered)


if __name__ == "__main__":
    unittest.main()
