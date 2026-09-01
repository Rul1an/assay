#!/usr/bin/env python3
import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
import time
import unittest
from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
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
    def test_platform_coverage_tracks_archive_version_not_source_pin(self):
        module = load_module()
        source = (ROOT / "README.md").read_text(encoding="utf-8")
        source = re.sub(
            r"Published v[^ ]+ CLI archives cover",
            "Published v5.4.0 CLI archives cover",
            source,
        )
        source += "\nHistorical note: v5.4.0 shipped earlier.\n"
        for version in ("5.5.2", "5.6.0-rc.1"):
            with self.subTest(version=version):
                rendered = module.render_release_readme(source, version)
                self.assertIn(f"Published v{version} CLI archives cover", rendered)
                self.assertNotIn("Published v5.4.0 CLI archives cover", rendered)
                self.assertIn("Historical note: v5.4.0 shipped earlier.", rendered)
                self.assertEqual(
                    module.render_release_readme(source + "\n", version), rendered + "\n"
                )

    def test_renderer_refuses_missing_or_duplicate_platform_coverage(self):
        module = load_module()
        source = (ROOT / "README.md").read_text(encoding="utf-8")
        coverage = next(
            line for line in source.splitlines(True) if " CLI archives cover " in line
        )
        for altered in (source.replace(coverage, ""), source + coverage):
            with self.subTest(altered=altered[-120:]):
                with self.assertRaisesRegex(ValueError, "exactly one platform-coverage sentence"):
                    module.render_release_readme(altered, "5.6.0")

    def test_cli_refuses_two_coverage_claims_on_one_line(self):
        source = (ROOT / "README.md").read_text(encoding="utf-8")
        coverage = next(line for line in source.splitlines() if " CLI archives cover " in line)
        current = re.search(r"Published v([^ ]+)", coverage).group(1)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = root / "scripts/ci/release_readme.py"
            script.parent.mkdir(parents=True)
            script.write_bytes(MODULE_PATH.read_bytes())
            for relative in load_module().PACKED_SOURCE_PATHS:
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((ROOT / relative).read_bytes())
            for control in (source, source + "\nHistorical note: v5.0.0 shipped earlier.\n"):
                (root / "README.md").write_text(control, encoding="utf-8")
                result = subprocess.run(
                    [sys.executable, str(script), "v5.6.0", "--assembled-cwd"],
                    cwd=root, capture_output=True, text=True, encoding="utf-8", timeout=10,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("Published v5.6.0 CLI archives cover", result.stdout)
            for second_version in (current, "5.0.0"):
                with self.subTest(second_version=second_version):
                    (root / "README.md").write_text(
                        source.replace(
                            coverage,
                            coverage + f" Published v{second_version} CLI archives cover obsolete.",
                        ),
                        encoding="utf-8",
                    )
                    result = subprocess.run(
                        [sys.executable, str(script), "v5.6.0", "--assembled-cwd"],
                        cwd=root, capture_output=True, text=True, encoding="utf-8", timeout=10,
                    )
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertIn("exactly one platform-coverage sentence", result.stderr)
                    self.assertEqual(result.stdout, "")

    def test_renderer_moves_only_active_release_claims_to_assembled_version(self):
        module = load_module()
        source = """# Assay

```bash
cargo install assay-cli --version 5.4.0 --locked
```

Current release: [`v5.4.0`](https://github.com/Rul1an/assay/releases/tag/v5.4.0).
- Published v5.4.0 CLI archives cover Linux x86_64/arm64.

For v5.5.2, run the last command from a source checkout or an extracted published CLI archive.
The installer is binary-only and does not carry the bounded quickstart assets.

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
- Published v5.4.0 CLI archives cover Linux x86_64/arm64.
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
            encoding="utf-8",
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("--version 5.5.0", completed.stdout)
        self.assertIn("releases/tag/v5.5.0", completed.stdout)
        self.assertIn("Published v5.5.0 CLI archives cover", completed.stdout)

    def test_cli_refuses_a_bare_version_that_the_release_contract_never_emits(self):
        completed = subprocess.run(
            [sys.executable, str(MODULE_PATH), "5.5.0"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
        )
        self.assertEqual(completed.returncode, 2)

    def test_cli_preserves_the_release_contract_prerelease_suffix(self):
        completed = subprocess.run(
            [sys.executable, str(MODULE_PATH), "v5.5.0-rc.1"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
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
            encoding="utf-8",
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
            'release_readme.py" "$VERSION" --assembled-cwd',
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

    def test_readmes_split_installer_from_published_cli_archives(self):
        root_readme = (ROOT / "README.md").read_text(encoding="utf-8")
        example_readme = (ROOT / "examples/mcp-quickstart/README.md").read_text(
            encoding="utf-8"
        )
        module = load_module()

        self.assertIn(CHECKOUT_SENTENCE, root_readme)
        self.assertIn(
            "The installer is binary-only and does not carry the bounded quickstart assets.",
            root_readme,
        )
        self.assertNotIn(
            "published v5.4.0 CLI archive install the binary but do not carry",
            root_readme,
        )
        self.assertNotIn(ARCHIVE_ROOT_CLAIM, root_readme)
        self.assertNotIn(ARCHIVE_ROOT_CLAIM, example_readme)
        self.assertIn(
            "source checkout or an extracted published CLI archive",
            example_readme,
        )
        self.assertIn(
            "The installer is binary-only and does not carry this bounded quickstart",
            example_readme,
        )
        self.assertEqual(len(module.CHECKOUT_RE.findall(root_readme)), 1)
        self.assertEqual(len(module.CHECKOUT_RE.findall(example_readme)), 0)




PACKED_SOURCE_MEMBERS = (
    ("binary", "${{ matrix.artifact }}"),
    ("license", "LICENSE"),
    ("quickstart-policy", "examples/mcp-quickstart/policy.yaml"),
    ("quickstart-run", "examples/mcp-quickstart/run.py"),
    ("quickstart-mock", "examples/mcp-quickstart/mock_server.py"),
)
CHECKOUT_SENTENCE = (
    "For v5.5.2, run the last command from a source checkout or an extracted published CLI archive."
)
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
        self.assertIn('release_readme.py" "$VERSION" --assembled-cwd', unix)
        self.assertIn('release_readme.py" "$VERSION" --assembled-cwd', windows)
        for name, fragment in PACKED_SOURCE_MEMBERS:
            with self.subTest(platform="unix", member=name):
                self.assertIn(fragment, unix)
            with self.subTest(platform="windows", member=name):
                self.assertIn(fragment, windows)


class ReleaseArchiveReadmeContract(unittest.TestCase):
    def test_source_readme_keeps_checkout_or_extracted_archive_wording(self):
        source = (ROOT / "README.md").read_text(encoding="utf-8")
        self.assertIn(CHECKOUT_SENTENCE, source)
        self.assertNotIn(ARCHIVE_ROOT_CLAIM, source)
        self.assertIn(
            "The installer is binary-only and does not carry the bounded quickstart assets.",
            source,
        )

    def test_rendered_readme_rewrites_absent_links_and_states_packed_quickstart(self):
        module = load_module()
        source = (ROOT / "README.md").read_text(encoding="utf-8")
        rendered = module.render_release_readme(source, "5.5.0")
        self.assertNotIn(CHECKOUT_SENTENCE, rendered)
        self.assertIn(module.ARCHIVE_QUICKSTART, rendered)
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
            "(https://raw.githubusercontent.com/Rul1an/assay/v5.5.0/demo/output/screenshots/mcp-wrap-demo.svg)",
            rendered,
        )
        self.assertNotIn(
            "github.com/Rul1an/assay/blob/v5.5.0/demo/output/screenshots/mcp-wrap-demo.svg",
            rendered,
        )
        self.assertNotIn("github.com/Rul1an/assay/blob/main/", rendered)
        self.assertNotIn("github.com/Rul1an/assay/tree/main/", rendered)
        self.assertNotIn("github.com/Rul1an/assay/raw/main/", rendered)
        self.assertIn('href="LICENSE"', rendered)
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
- Published v5.4.0 CLI archives cover Linux x86_64/arm64.

For v5.5.2, run the last command from a source checkout or an extracted published CLI archive.
The installer is binary-only and does not carry the bounded quickstart assets.

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
- Published v5.4.0 CLI archives cover Linux x86_64/arm64.
"""
        with self.assertRaisesRegex(ValueError, "exactly one published-checkout sentence"):
            rendered = module.render_release_readme(source, "5.5.0")
            returned_without_archive_truth = ARCHIVE_ROOT_CLAIM not in rendered
            self.fail(f"returned_without_archive_truth={returned_without_archive_truth}")

    def test_renderer_refuses_ambiguous_checkout_sentence(self):
        module = load_module()
        source = """cargo install assay-cli --version 5.4.0 --locked
Current release: [`v5.4.0`](https://github.com/Rul1an/assay/releases/tag/v5.4.0).
- Published v5.4.0 CLI archives cover Linux x86_64/arm64.
For v5.5.2, run the last command from a source checkout or an extracted published CLI archive.
The installer is binary-only and does not carry the bounded quickstart assets.
For v5.5.2, run the last command from a source checkout or an extracted published CLI archive.
The installer is binary-only and does not carry the bounded quickstart assets.
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
            "- Published v5.4.0 CLI archives cover Linux x86_64/arm64.\n"
            "For v5.5.2, run the last command from a source checkout or an extracted published CLI archive.\n"
            "The installer is binary-only and does not carry the bounded quickstart assets.\n"
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

PINNED_SOURCE = (
    "cargo install assay-cli --version 5.4.0 --locked\n"
    "Current release: [`v5.4.0`](https://github.com/Rul1an/assay/releases/tag/v5.4.0).\n"
    "- Published v5.4.0 CLI archives cover Linux x86_64/arm64.\n"
    "For v5.5.2, run the last command from a source checkout or an extracted published CLI archive.\n"
    "The installer is binary-only and does not carry the bounded quickstart assets.\n"
)
PACKED_MEMBER_PATHS = frozenset(
    {
        "LICENSE",
        "examples/mcp-quickstart",
        "examples/mcp-quickstart/",
        "examples/mcp-quickstart/policy.yaml",
        "examples/mcp-quickstart/run.py",
        "examples/mcp-quickstart/mock_server.py",
    }
)


class ReleaseArchiveReadmeFollowOn(unittest.TestCase):
    def test_renderer_rewrites_first_party_mutable_main_ref(self):
        module = load_module()
        source = PINNED_SOURCE + (
            '<a href="https://github.com/Rul1an/assay/blob/main/LICENSE">'
            "license</a>\n"
            "[tree](https://github.com/Rul1an/assay/tree/HEAD/docs/)\n"
            "[raw](https://raw.githubusercontent.com/Rul1an/assay/master/CHANGELOG.md)\n"
        )
        rendered = module.render_release_readme(source, "5.5.0")
        self.assertNotIn("/blob/main/", rendered)
        self.assertNotIn("/tree/HEAD/", rendered)
        self.assertNotIn("/master/", rendered)
        self.assertIn('href="LICENSE"', rendered)
        self.assertIn(
            "[tree](https://github.com/Rul1an/assay/tree/v5.5.0/docs/)",
            rendered,
        )
        self.assertIn(
            "[raw](https://github.com/Rul1an/assay/blob/v5.5.0/CHANGELOG.md)",
            rendered,
        )

    def test_renderer_uses_raw_tag_urls_for_media(self):
        module = load_module()
        source = PINNED_SOURCE + (
            "![demo](demo/output/screenshots/mcp-wrap-demo.svg)\n"
            "![gif](examples/privileged-action-gate/demo.gif)\n"
        )
        rendered = module.render_release_readme(source, "5.5.0")
        self.assertIn(
            "![demo](https://raw.githubusercontent.com/Rul1an/assay/v5.5.0/demo/output/screenshots/mcp-wrap-demo.svg)",
            rendered,
        )
        self.assertIn(
            "![gif](https://raw.githubusercontent.com/Rul1an/assay/v5.5.0/examples/privileged-action-gate/demo.gif)",
            rendered,
        )
        self.assertNotIn("github.com/Rul1an/assay/blob/", rendered)

    def test_renderer_refuses_a_nonexistent_quickstart_member(self):
        module = load_module()
        source = PINNED_SOURCE + "[missing](examples/mcp-quickstart/not-packed.py)\n"
        with self.assertRaisesRegex(ValueError, "assembled archive member"):
            module.render_release_readme(source, "5.5.0")

    def test_renderer_refuses_a_github_workflow_path(self):
        module = load_module()
        source = PINNED_SOURCE + "[ci](.github/workflows/ci.yml)\n"
        with self.assertRaisesRegex(ValueError, "unclassifiable"):
            module.render_release_readme(source, "5.5.0")

    def test_renderer_refuses_a_reference_style_link(self):
        module = load_module()
        source = PINNED_SOURCE + "[see][docs]\n\n[docs]: docs/concepts/scope.md\n"
        with self.assertRaisesRegex(ValueError, "unclassifiable"):
            module.render_release_readme(source, "5.5.0")

    def test_renderer_refuses_an_overlong_link_label(self):
        module = load_module()
        source = PINNED_SOURCE + "[" + ("A" * 600) + "](docs/concepts/scope.md)\n"
        with self.assertRaisesRegex(ValueError, "unclassifiable"):
            module.render_release_readme(source, "5.5.0")

    def test_renderer_rewrites_html_img_src(self):
        module = load_module()
        source = PINNED_SOURCE + (
            '<img src="demo/output/screenshots/mcp-wrap-demo.svg" alt="demo">\n'
        )
        rendered = module.render_release_readme(source, "5.5.0")
        self.assertIn(
            'src="https://raw.githubusercontent.com/Rul1an/assay/v5.5.0/demo/output/screenshots/mcp-wrap-demo.svg"',
            rendered,
        )

    def test_renderer_rewrites_single_quoted_href(self):
        module = load_module()
        source = PINNED_SOURCE + "<a href='docs/concepts/scope.md'>scope</a>\n"
        rendered = module.render_release_readme(source, "5.5.0")
        self.assertIn(
            "href='https://github.com/Rul1an/assay/blob/v5.5.0/docs/concepts/scope.md'",
            rendered,
        )

    def test_renderer_peels_dot_slash_and_rejects_traversal(self):
        module = load_module()
        source = PINNED_SOURCE + "[ok](./LICENSE)\n"
        rendered = module.render_release_readme(source, "5.5.0")
        self.assertIn("[ok](LICENSE)", rendered)
        self.assertNotIn("lstrip(", (ROOT / "scripts/ci/release_readme.py").read_text(encoding="utf-8"))
        for hostile in ("../LICENSE", "/LICENSE", ".../LICENSE"):
            with self.subTest(path=hostile):
                with self.assertRaisesRegex(ValueError, "traversal|unclassifiable"):
                    module.render_release_readme(
                        PINNED_SOURCE + f"[x]({hostile})\n", "5.5.0"
                    )

    def test_package_steps_assemble_members_before_rendering(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
        for heading in ("Package (Unix)", "Package (Windows)"):
            step = package_step(workflow, heading)
            render_at = step.find("release_readme.py")
            self.assertGreater(render_at, 0, heading)
            for name, fragment in PACKED_SOURCE_MEMBERS:
                copy_at = step.find(fragment)
                self.assertGreater(copy_at, -1, f"{heading} missing {name}")
                self.assertLess(copy_at, render_at, f"{heading} {name} copied after render")

    def test_renderer_cli_rejects_arbitrary_archive_path_before_filesystem_access(self):
        module = load_module()
        with mock.patch.object(module, "list_archive_members") as list_members:
            with redirect_stderr(StringIO()):
                result = module.main(["v5.5.0", "/tmp/attacker-selected-archive"])
        self.assertEqual(result, 2)
        list_members.assert_not_called()

    def test_renderer_cli_assembled_mode_reads_only_the_current_directory(self):
        module = load_module()
        cwd = Path("/assembled/archive")
        members = module.default_packed_members()
        with mock.patch.object(module.Path, "cwd", return_value=cwd), mock.patch.object(
            module, "list_archive_members", return_value=members
        ) as list_members, redirect_stdout(StringIO()):
            result = module.main(["v5.5.0", "--assembled-cwd"])
        self.assertEqual(result, 0)
        list_members.assert_called_once_with(cwd)

    def test_package_steps_enter_the_assembled_directory_for_rendering(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
        unix = package_step(workflow, "Package (Unix)")
        windows = package_step(workflow, "Package (Windows)")
        self.assertIn('cd "dist/${ARCHIVE_NAME}"', unix)
        self.assertIn("--assembled-cwd", unix)
        self.assertIn('Push-Location "dist\\${ARCHIVE_NAME}"', windows)
        self.assertIn("--assembled-cwd", windows)

    def test_unix_and_windows_assembled_member_sets_render(self):
        module = load_module()
        source = (ROOT / "README.md").read_text(encoding="utf-8")
        members = module.expand_archive_members(
            {
                "assay",
                "LICENSE",
                "examples/mcp-quickstart/policy.yaml",
                "examples/mcp-quickstart/run.py",
                "examples/mcp-quickstart/mock_server.py",
            }
        )
        self.assertTrue(PACKED_MEMBER_PATHS <= members)
        rendered = module.render_release_readme(source, "5.5.0", members=members)
        self.assertIn("[MIT](LICENSE)", rendered)
        self.assertIn(
            "https://raw.githubusercontent.com/Rul1an/assay/v5.5.0/demo/output/screenshots/mcp-wrap-demo.svg",
            rendered,
        )
        self.assertNotRegex(rendered, r"github\.com/Rul1an/assay/(blob|tree|raw)/(HEAD|main|master)/")
        self.assertNotRegex(
            rendered, r"raw\.githubusercontent\.com/Rul1an/assay/(HEAD|main|master)/"
        )

    def test_kernel_matrix_paths_include_readme(self):
        workflow = (ROOT / ".github/workflows/kernel-matrix.yml").read_text(encoding="utf-8")
        paths = workflow.split("paths:", 1)[1].split("workflow_dispatch", 1)[0]
        self.assertIn('"README.md"', paths)


# Present in the source README and outside Latin-1 / cp1252.
OUTSIDE_LATIN1 = ("—", "✅", "📋")


def _assemble_packed_cwd(directory: Path) -> Path:
    for relative in load_module().PACKED_SOURCE_PATHS:
        source = ROOT / relative
        destination = directory / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(source.read_bytes())
    return directory


def _render_assembled_readme(encoding: str) -> tuple[int, bytes, bytes]:
    with tempfile.TemporaryDirectory() as raw:
        assembled = _assemble_packed_cwd(Path(raw))
        readme = assembled / "README.md"
        env = os.environ.copy()
        env["PYTHONIOENCODING"] = encoding
        env["PYTHONUTF8"] = "0"
        with readme.open("wb") as stdout:
            completed = subprocess.run(
                [sys.executable, str(MODULE_PATH), "v5.5.0", "--assembled-cwd"],
                cwd=assembled,
                env=env,
                stdout=stdout,
                stderr=subprocess.PIPE,
                check=False,
            )
        return completed.returncode, readme.read_bytes(), completed.stderr


class ReleaseReadmeStdoutEncoding(unittest.TestCase):
    def test_assembled_cwd_emits_complete_utf8_under_inherited_stdout_encodings(self):
        for encoding in ("cp1252", "ascii", "utf-8"):
            with self.subTest(encoding=encoding):
                code, raw, stderr = _render_assembled_readme(encoding)
                self.assertEqual(code, 0, stderr)
                self.assertGreater(len(raw), 0)
                rendered = raw.decode("utf-8")
                self.assertIn(ARCHIVE_ROOT_CLAIM, rendered)
                self.assertIn(QUICKSTART_COMMAND, rendered)
                for marker in OUTSIDE_LATIN1:
                    self.assertIn(marker, rendered)
                self.assertNotIn("\ufffd", rendered)


if __name__ == "__main__":
    unittest.main()
