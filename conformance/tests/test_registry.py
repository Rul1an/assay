#!/usr/bin/env python3
"""Canonical conformance registry: mutations, policy, INDEX, hostile load.

    python3 -W error::ResourceWarning conformance/tests/test_registry.py

The runnable inventory lives in one file. Deleting a published suite from it,
or adding a published root without registering it, must fail the product gate.
Adequacy manifests (`*.manifest.json`) are a different domain and are not this
inventory.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "conformance"))

import registry  # noqa: E402
import run_all  # noqa: E402

CI_YML = REPO / ".github/workflows/ci.yml"
STANDALONE = REPO / ".github/workflows/conformance-inventory.yml"
INVENTORY_STEP_RE = re.compile(r"(?ms)^      - name: Conformance inventory\n(?:        .+\n)+")
JOB_KEY_RE = re.compile(r"^  [A-Za-z0-9_][A-Za-z0-9_-]*:")


def _scope_job(text: str) -> str:
    lines = text.splitlines(keepends=True)
    start = next((i for i, line in enumerate(lines) if line.startswith("  scope:")), None)
    if start is None:
        raise AssertionError("ci.yml missing scope job")
    end = start + 1
    while end < len(lines) and not JOB_KEY_RE.match(lines[end]):
        end += 1
    return "".join(lines[start:end])


def _inventory_step(text: str) -> str:
    step = INVENTORY_STEP_RE.search(_scope_job(text))
    if step is None:
        raise AssertionError("scope job missing Conformance inventory step")
    return step.group(0)
POLICIES = ("required", "optional", "external-candidate")


class CanonicalFile(unittest.TestCase):
    def test_one_registry_file_exists(self):
        self.assertTrue(registry.REGISTRY_PATH.is_file(), registry.REGISTRY_PATH)

    def test_run_all_does_not_hand_copy_a_suites_list(self):
        source = Path(run_all.__file__).read_text()
        self.assertNotIn("SUITES = [", source)

    def test_runner_suites_are_the_registry_suites(self):
        loaded = [s["id"] for s in registry.load_suites()]
        bound = [s["id"] for s in run_all.SUITES]
        self.assertEqual(bound, loaded)
        self.assertEqual(len(bound), 6)

    def test_adequacy_manifests_are_a_different_id_space(self):
        manifests = sorted(
            p.name[: -len(".manifest.json")]
            for p in (REPO / "conformance/adequacy").glob("*.manifest.json")
        )
        suite_ids = sorted(s["id"] for s in registry.load_suites())
        self.assertNotEqual(manifests, suite_ids)
        self.assertEqual(len(manifests), 5)
        self.assertEqual(len(suite_ids), 6)


class PolicyAndMachineOutput(unittest.TestCase):
    def test_every_suite_declares_an_explicit_policy(self):
        for suite in registry.load_suites():
            self.assertIn(suite["policy"], POLICIES, suite["id"])

    def test_external_candidate_is_not_called_complete(self):
        suites = registry.load_suites()
        self.assertTrue(any(s["policy"] == "external-candidate" for s in suites))
        p = subprocess.run(
            [sys.executable, str(run_all.__file__), "--json"],
            capture_output=True, text=True, timeout=300)
        report = json.loads(p.stdout)
        self.assertIn("declared", report)
        self.assertIn("executed", report)
        self.assertIn("complete", report)
        self.assertIs(report["complete"], False)
        non_run = [s for s in report["suites"] if s["grade"] not in run_all.EXECUTED_GRADES]
        self.assertGreater(len(non_run), 0)
        for row in non_run:
            self.assertTrue(row["detail"], row)


class CompletenessMutations(unittest.TestCase):
    """RED on the old hand list: delete-one and add-unregistered must bite."""

    def test_deleting_one_registered_published_suite_fails_the_gate(self):
        doc = registry.load_registry()
        before = [s["id"] for s in doc["suites"]]
        self.assertIn("privileged-mcp-action-v0", before)
        doc["suites"] = [s for s in doc["suites"] if s["id"] != "privileged-mcp-action-v0"]
        with tempfile.TemporaryDirectory() as raw:
            mutated = Path(raw) / "registry.json"
            mutated.write_text(json.dumps(doc), encoding="utf-8")
            reasons = registry.registry_completeness_reasons(
                REPO, registry_path=mutated)
        self.assertTrue(reasons, "deleting a published suite must not stay clean")
        self.assertTrue(
            any("privileged-mcp-action-v0" in r or "unregistered" in r for r in reasons),
            reasons)

    def test_adding_an_unregistered_published_root_fails_the_gate(self):
        root = REPO / "conformance" / "_unpublished_mutation_root_"
        if root.exists():
            shutil.rmtree(root)
        try:
            root.mkdir()
            (root / "MANIFEST.json").write_text("{}\n", encoding="utf-8")
            reasons = registry.registry_completeness_reasons(REPO)
            self.assertTrue(reasons, "an unregistered published root must not pass")
            self.assertTrue(
                any("_unpublished_mutation_root_" in r for r in reasons),
                reasons)
        finally:
            if root.exists():
                shutil.rmtree(root)

    def test_the_real_tree_is_green(self):
        self.assertEqual(registry.registry_completeness_reasons(REPO), [])


class IndexProjection(unittest.TestCase):
    def test_index_inventory_table_is_the_registry_render(self):
        suites = registry.load_suites()
        index = (REPO / "conformance/INDEX.md").read_text(encoding="utf-8")
        rendered = registry.render_inventory_table(suites)
        section = registry.index_inventory_section(index)
        self.assertEqual(section, rendered)
        for suite in suites:
            self.assertIn(suite["index_corpus"], section, suite["id"])

    def test_index_does_not_advertise_a_corpus_the_registry_omits(self):
        reasons = registry.index_reasons(REPO, registry.load_suites())
        self.assertEqual(reasons, [])


class HostileLoader(unittest.TestCase):
    def _write(self, payload, *, size=None):
        path = Path(self.raw) / "registry.json"
        if size is not None:
            path.write_bytes(b"{" + b"x" * size)
        elif isinstance(payload, (bytes, bytearray)):
            path.write_bytes(payload)
        else:
            path.write_text(payload if isinstance(payload, str) else json.dumps(payload),
                            encoding="utf-8")
        return path

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.raw = self._tmp.name

    def tearDown(self):
        self._tmp.cleanup()

    def test_missing_registry_is_not_a_pass(self):
        missing = Path(self.raw) / "no-such.json"
        with self.assertRaises(registry.RegistryError):
            registry.load_registry(missing)
        reasons = registry.registry_completeness_reasons(
            REPO, registry_path=missing)
        self.assertTrue(reasons)
        self.assertTrue(any("missing" in r or "absent" in r for r in reasons))

    def test_malformed_json_is_rejected(self):
        path = self._write("{")
        with self.assertRaises(registry.RegistryError):
            registry.load_registry(path)

    def test_huge_file_is_rejected(self):
        path = self._write(None, size=registry.MAX_REGISTRY_BYTES + 1)
        with self.assertRaises(registry.RegistryError) as ctx:
            registry.load_registry(path)
        self.assertIn("exceeds", str(ctx.exception))

    def test_unexpected_type_is_rejected(self):
        path = self._write({"schema": registry.SCHEMA, "suites": "nope"})
        with self.assertRaises(registry.RegistryError):
            registry.load_registry(path)

    def test_duplicate_ids_are_rejected(self):
        suite = registry.load_suites()[0]
        path = self._write({"schema": registry.SCHEMA, "suites": [suite, dict(suite)]})
        with self.assertRaises(registry.RegistryError) as ctx:
            registry.load_registry(path)
        self.assertIn("duplicate", str(ctx.exception))

    def test_missing_required_field_is_rejected(self):
        suite = {k: v for k, v in registry.load_suites()[0].items() if k != "policy"}
        path = self._write({"schema": registry.SCHEMA, "suites": [suite]})
        with self.assertRaises(registry.RegistryError) as ctx:
            registry.load_registry(path)
        self.assertIn("policy", str(ctx.exception))

    def test_path_escape_is_rejected(self):
        suite = dict(registry.load_suites()[0])
        suite["path"] = "../../../etc/passwd"
        path = self._write({"schema": registry.SCHEMA, "suites": [suite]})
        with self.assertRaises(registry.RegistryError) as ctx:
            registry.load_registry(path)
        self.assertIn("path", str(ctx.exception).lower())

    def test_symlink_outside_the_repo_is_rejected(self):
        outside = Path(self.raw) / "outside.json"
        outside.write_text("{}", encoding="utf-8")
        link = REPO / "conformance" / "_symlink_mutation_registry.json"
        if link.exists() or link.is_symlink():
            link.unlink()
        try:
            os.symlink(outside, link)
            with self.assertRaises(registry.RegistryError) as ctx:
                registry.load_registry(link)
            self.assertIn("symlink", str(ctx.exception).lower())
        finally:
            if link.exists() or link.is_symlink():
                link.unlink()


class ProductWorkflow(unittest.TestCase):
    def test_no_separate_inventory_workflow(self):
        self.assertFalse(STANDALONE.exists(), STANDALONE)

    def test_scope_job_invokes_require_complete_as_a_hard_check(self):
        step = _inventory_step(CI_YML.read_text(encoding="utf-8"))
        self.assertIn("python3 conformance/registry.py", step)
        self.assertIn("conformance/run_all.py --require-complete --completion-scope required", step)
        self.assertNotIn("continue-on-error", step)

    def test_deleting_the_scope_job_callsite_fails_this_test(self):
        text = CI_YML.read_text(encoding="utf-8")
        with self.assertRaises(AssertionError):
            _inventory_step(INVENTORY_STEP_RE.sub("", text))

    def test_deleting_the_require_complete_callsite_fails_this_test(self):
        step = _inventory_step(CI_YML.read_text(encoding="utf-8"))
        mutated = step.replace("--require-complete", "")
        self.assertNotEqual(mutated, step)
        self.assertNotIn("--require-complete", mutated)


class RegistryDoesNotRunAll(unittest.TestCase):
    def test_registry_py_does_not_invoke_or_neutralize_run_all(self):
        source = Path(registry.__file__).read_text(encoding="utf-8")
        self.assertNotIn("subprocess", source)
        self.assertNotIn("--require-complete", source)
        self.assertNotIn("returncode in (0, 3)", source)


if __name__ == "__main__":
    unittest.main(verbosity=2)
