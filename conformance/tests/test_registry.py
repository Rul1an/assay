#!/usr/bin/env python3
"""Canonical conformance registry: mutations, policy, INDEX, hostile load.

    python3 -W error::ResourceWarning conformance/tests/test_registry.py

The runnable inventory lives in one file. Deleting a published suite from it,
or adding a published root without registering it, must fail the product gate.
Adequacy manifests (`*.manifest.json`) are a different domain and are not this
inventory.
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "conformance"))
sys.path.insert(0, str(REPO / "conformance/tests"))

import registry  # noqa: E402
import run_all  # noqa: E402
from test_completion_scope import (  # noqa: E402
    GATED_LINUX_JOB,
    GATED_LINUX_STEP,
    PLAIN_RUN_ALL,
    REVISION_WITNESS,
    assert_hard_run_command,
    named_job,
    named_step,
    trusted_prefix_mutations,
)

CI_YML = REPO / ".github/workflows/ci.yml"
STANDALONE = REPO / ".github/workflows/conformance-inventory.yml"
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
        self.assertEqual(len(bound), 10)

    def test_adequacy_manifests_are_a_different_id_space(self):
        manifests = sorted(
            p.name[: -len(".manifest.json")]
            for p in (REPO / "conformance/adequacy").glob("*.manifest.json")
        )
        suite_ids = sorted(s["id"] for s in registry.load_suites())
        self.assertNotEqual(manifests, suite_ids)
        self.assertEqual(len(manifests), 5)
        self.assertEqual(len(suite_ids), 10)


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
    """Presence markers are an independent oracle. Registry stays the metadata source."""

    def test_each_local_root_delete_is_caught_while_its_marker_remains(self):
        suites = registry.load_suites()
        local = [s for s in suites if not s["path"].startswith(("http://", "https://"))]
        paths = sorted({s["path"] for s in local})
        self.assertEqual(len(paths), 5, paths)
        for path in paths:
            with self.subTest(path):
                marker = REPO / registry.sidecar_rel(path)
                self.assertTrue(marker.is_file(), marker)
                self.assertFalse(marker.is_symlink(), marker)
                self.assertFalse((REPO / path / ".assay-conformance-root").exists())
                doc = registry.load_registry()
                doc["suites"] = [s for s in doc["suites"] if s["path"] != path]
                with tempfile.TemporaryDirectory() as raw:
                    mutated = Path(raw) / "registry.json"
                    mutated.write_text(json.dumps(doc), encoding="utf-8")
                    reasons = registry.registry_completeness_reasons(
                        REPO, registry_path=mutated)
                self.assertTrue(reasons, "deleting root %s must not stay clean" % path)
                self.assertTrue(
                    any("unregistered published root" in r and path in r
                        for r in reasons),
                    reasons)

    def test_required_local_lane_ids_are_an_independent_registry_contract(self):
        pinned = frozenset((
            "privileged-mcp-action-producer",
            "privileged-mcp-action-verifier",
            "privileged-mcp-action-e2e",
        ))
        self.assertEqual(registry.REQUIRED_LOCAL_LANE_IDS, pinned)
        loaded = {s["id"] for s in registry.load_suites()}
        bound = {s["id"] for s in run_all.SUITES}
        self.assertEqual(pinned & loaded, pinned)
        self.assertEqual(pinned & bound, pinned)
        self.assertNotIn("privileged-mcp-action-projection", pinned)
        self.assertNotIn("privileged-mcp-action-v0", pinned)

    def test_deleting_a_required_local_lane_row_is_a_registry_contract_error(self):
        doc = registry.load_registry()
        doc["suites"] = [
            s for s in doc["suites"] if s["id"] != "privileged-mcp-action-producer"
        ]
        with tempfile.TemporaryDirectory() as raw:
            mutated = Path(raw) / "registry.json"
            mutated.write_text(json.dumps(doc), encoding="utf-8")
            reasons = registry.registry_completeness_reasons(
                REPO, registry_path=mutated)
        self.assertTrue(
            any("required local lane missing: privileged-mcp-action-producer" in r
                for r in reasons),
            reasons)
        self.assertFalse(
            any("unproved" in r.lower() for r in reasons),
            reasons)

    def test_adding_an_unregistered_marker_fails_the_gate(self):
        sidecar = REPO / registry.sidecar_rel("conformance/_unpublished_mutation_root_")
        if sidecar.exists() or sidecar.is_symlink():
            sidecar.unlink()
        try:
            sidecar.write_bytes(b"")
            reasons = registry.registry_completeness_reasons(REPO)
            self.assertTrue(reasons, "an unregistered marker must not pass")
            self.assertTrue(
                any("_unpublished_mutation_root_" in r for r in reasons),
                reasons)
        finally:
            if sidecar.exists() or sidecar.is_symlink():
                sidecar.unlink()

    def test_discovery_does_not_follow_symlink_directories(self):
        outside = Path(tempfile.mkdtemp(prefix="assay-marker-"))
        link = REPO / "conformance" / "_symlink_root_"
        if link.exists() or link.is_symlink():
            if link.is_symlink() or link.is_file():
                link.unlink()
            else:
                shutil.rmtree(link)
        try:
            (outside / "nested.assay-conformance-root").write_bytes(b"")
            os.symlink(outside, link)
            roots = registry.discover_published_roots(REPO)
            self.assertNotIn("conformance/_symlink_root_/nested", roots)
            self.assertNotIn("conformance/_symlink_root_", roots)
        finally:
            if link.exists() or link.is_symlink():
                link.unlink()
            shutil.rmtree(outside, ignore_errors=True)

    def test_marker_hardening_does_not_change_digest_pinned_files(self):
        pinned = {
            "examples/mcp-jsonrpc-id-conformance/SHA256SUMS":
                "19443e513ea29b1d83c8f184fae65a056cd7c50f2f0fef9a9a54c224673ae5ac",
            "conformance/privileged-mcp-action-v0/MANIFEST.json":
                "15a726d1ecd3dec624d0224c5137921f55962a6dcdeefba2cf85041356858569",
            "conformance/privileged-mcp-action-v1/MANIFEST.json":
                "dafa33394e1d93869dbd17bdde3b6191ca237598429134c951aeec4e916a35b3",
            "crates/assay-canonical/tests/vectors/rfc8785.json":
                "64a71a5e26fc51918b77420c2b6b9b487de2ddd6ee8aa1ce7f3d9b55403a5c20",
        }
        for rel, expected in pinned.items():
            data = (REPO / rel).read_bytes()
            self.assertEqual(hashlib.sha256(data).hexdigest(), expected, rel)
            self.assertNotIn(b"assay-conformance-root", data)
        local = [s for s in registry.load_suites()
                 if not s["path"].startswith(("http://", "https://"))]
        for suite in local:
            root = REPO / suite["path"]
            self.assertFalse((root / ".assay-conformance-root").exists(), root)
            self.assertTrue((REPO / registry.sidecar_rel(suite["path"])).is_file())

    def test_the_real_tree_is_green(self):
        self.assertEqual(registry.registry_completeness_reasons(REPO), [])
        self.assertEqual(
            sorted(registry.discover_published_roots(REPO)),
            sorted({s["path"] for s in registry.load_suites()
                    if not s["path"].startswith(("http://", "https://"))}))


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

    def test_index_does_not_call_declared_inventory_all_six(self):
        declared = len(registry.load_suites())
        self.assertEqual(declared, 10)
        for rel in ("conformance/INDEX.md.in", "conformance/INDEX.md"):
            text = (REPO / rel).read_text(encoding="utf-8")
            self.assertNotIn("all-six fact", text, rel)
            self.assertIn("(10)", text, rel)


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

    def test_empty_test_filter_on_required_cargo_is_rejected(self):
        """Canonical mutation: required cargo test_filter="" must go red."""
        suites = [dict(s) for s in registry.load_suites()]
        producer = next(s for s in suites if s["id"] == "privileged-mcp-action-producer")
        producer["test_filter"] = ""
        path = self._write({"schema": registry.SCHEMA, "suites": suites})
        with self.assertRaises(registry.RegistryError) as ctx:
            registry.load_registry(path)
        self.assertIn("test_filter", str(ctx.exception))
        reasons = registry.registry_completeness_reasons(REPO, registry_path=path)
        self.assertTrue(reasons, "empty required test_filter must not stay clean")
        self.assertTrue(any("test_filter" in r for r in reasons), reasons)

    def test_missing_test_filter_on_required_cargo_is_rejected(self):
        suites = [dict(s) for s in registry.load_suites()]
        producer = next(s for s in suites if s["id"] == "privileged-mcp-action-producer")
        producer.pop("test_filter", None)
        path = self._write({"schema": registry.SCHEMA, "suites": suites})
        with self.assertRaises(registry.RegistryError) as ctx:
            registry.load_registry(path)
        self.assertIn("test_filter", str(ctx.exception))

    def test_optional_cargo_may_omit_test_filter(self):
        suite = next(s for s in registry.load_suites() if s["id"] == "rfc8785-canonicalization")
        self.assertEqual(suite["kind"], "cargo")
        self.assertEqual(suite["policy"], "optional")
        self.assertNotIn("test_filter", suite)

    def test_whitespace_only_test_filter_on_required_cargo_is_rejected(self):
        for filt in (" ", "\t", " \t\n"):
            with self.subTest(repr(filt)):
                suites = [dict(s) for s in registry.load_suites()]
                producer = next(s for s in suites if s["id"] == "privileged-mcp-action-producer")
                producer["test_filter"] = filt
                path = self._write({"schema": registry.SCHEMA, "suites": suites})
                with self.assertRaises(registry.RegistryError) as ctx:
                    registry.load_registry(path)
                self.assertIn("test_filter", str(ctx.exception))

    def test_usable_test_filter_is_the_shared_strip_truth(self):
        self.assertIsNone(registry.usable_test_filter(None))
        self.assertIsNone(registry.usable_test_filter(""))
        self.assertIsNone(registry.usable_test_filter(" "))
        self.assertIsNone(registry.usable_test_filter("\t"))
        self.assertIsNone(registry.usable_test_filter(" \t\n"))
        self.assertIsNone(registry.usable_test_filter(1))
        self.assertEqual(registry.usable_test_filter("producer_lane_"), "producer_lane_")
        self.assertEqual(registry.usable_test_filter("  producer_lane_  "), "producer_lane_")

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

    def test_bool_is_not_a_vectors_int(self):
        suite = dict(registry.load_suites()[0])
        suite["vectors"] = True
        path = self._write({"schema": registry.SCHEMA, "suites": [suite]})
        with self.assertRaises(registry.RegistryError) as ctx:
            registry.load_registry(path)
        self.assertIn("vectors", str(ctx.exception))

    def test_maturity_and_index_fields_must_be_strings(self):
        for field, value in (
            ("maturity", 1),
            ("index_corpus", 1),
            ("index_vectors", 3),
            ("index_runner", False),
            ("index_maturity", ["x"]),
        ):
            with self.subTest(field):
                suite = dict(registry.load_suites()[0])
                suite[field] = value
                path = self._write({"schema": registry.SCHEMA, "suites": [suite]})
                with self.assertRaises(registry.RegistryError) as ctx:
                    registry.load_registry(path)
                self.assertIn(field, str(ctx.exception))

    def test_local_suite_path_symlink_outside_the_repo_is_rejected(self):
        outside = Path(self.raw) / "outside_root"
        outside.mkdir()
        link = REPO / "conformance" / "_escape_suite_root_"
        if link.exists() or link.is_symlink():
            if link.is_symlink() or link.is_file():
                link.unlink()
            else:
                shutil.rmtree(link)
        try:
            os.symlink(outside, link)
            suite = dict(registry.load_suites()[0])
            suite["id"] = "escape-suite"
            suite["path"] = "conformance/_escape_suite_root_"
            path = self._write({"schema": registry.SCHEMA, "suites": [suite]})
            with self.assertRaises(registry.RegistryError) as ctx:
                registry.load_registry(path)
            self.assertIn("symlink", str(ctx.exception).lower())
        finally:
            if link.exists() or link.is_symlink():
                link.unlink()


class ProductWorkflow(unittest.TestCase):
    def test_no_separate_inventory_workflow(self):
        self.assertFalse(STANDALONE.exists(), STANDALONE)

    def test_scope_job_invokes_require_complete_as_a_hard_check(self):
        text = CI_YML.read_text(encoding="utf-8")
        step = assert_hard_run_command(
            text, "scope", "Conformance inventory")
        self.assertIn("python3 conformance/registry.py", step)

    def test_scope_job_invokes_completion_scope_suite(self):
        step = named_step(
            CI_YML.read_text(encoding="utf-8"),
            "scope",
            "Conformance inventory",
        )
        self.assertIn("conformance/tests/test_completion_scope.py", step)
        mutated = step.replace("conformance/tests/test_completion_scope.py", "")
        self.assertNotEqual(mutated, step)
        self.assertNotIn("conformance/tests/test_completion_scope.py", mutated)

    def test_deleting_the_scope_job_callsite_fails_this_test(self):
        text = CI_YML.read_text(encoding="utf-8")
        with self.assertRaises(AssertionError):
            named_step(
                text.replace(
                    "      - name: Conformance inventory\n",
                    "      - name: Deleted conformance inventory\n",
                    1,
                ),
                "scope",
                "Conformance inventory",
            )

    def test_deleting_the_require_complete_callsite_fails_this_test(self):
        step = named_step(
            CI_YML.read_text(encoding="utf-8"),
            GATED_LINUX_JOB,
            GATED_LINUX_STEP,
        )
        mutated = step.replace("--require-complete", "")
        self.assertNotEqual(mutated, step)
        self.assertNotIn("--require-complete", mutated)

    def test_commented_run_all_command_fails_the_inventory_guard(self):
        text = CI_YML.read_text(encoding="utf-8")
        mutated = text.replace(PLAIN_RUN_ALL, "# " + PLAIN_RUN_ALL, 1)
        with self.assertRaises(AssertionError):
            assert_hard_run_command(
                mutated, "scope", "Conformance inventory")

    def test_softened_run_all_command_fails_the_inventory_guard(self):
        text = CI_YML.read_text(encoding="utf-8")
        mutated = text.replace(PLAIN_RUN_ALL, PLAIN_RUN_ALL + " || true", 1)
        with self.assertRaises(AssertionError):
            assert_hard_run_command(
                mutated, "scope", "Conformance inventory")

    def test_removing_scope_revision_witness_fails(self):
        text = CI_YML.read_text(encoding="utf-8")
        step = named_step(text, "scope", "Conformance inventory")
        mutated_step = step.replace("          " + REVISION_WITNESS + "\n", "", 1)
        self.assertNotEqual(mutated_step, step)
        with self.assertRaises(AssertionError):
            assert_hard_run_command(
                text.replace(step, mutated_step, 1),
                "scope",
                "Conformance inventory",
            )

    def test_scope_workflow_document_shape_fails_closed(self):
        text = CI_YML.read_text(encoding="utf-8")
        env_mutations = (
            ("BASH_ENV", "  BASH_ENV: /tmp/assay-bash-env\n"),
            ("PATH", "  PATH: /tmp\n"),
            ("quoted GIT_DIR", "  'GIT_DIR': /tmp/repo\n"),
            ("unrelated env", "  UNRELATED_WORKFLOW_ENV: allowed\n"),
        )
        for label, addition in env_mutations:
            with self.subTest(label=label):
                mutated = text.replace("env:\n", "env:\n" + addition, 1)
                with self.assertRaises(AssertionError):
                    assert_hard_run_command(
                        mutated, "scope", "Conformance inventory")
        defaults = text.replace(
            "permissions: {}\n",
            "defaults:\n  run:\n    working-directory: /tmp\n\npermissions: {}\n",
            1,
        )
        self.assertNotEqual(defaults, text)
        with self.assertRaises(AssertionError):
            assert_hard_run_command(
                defaults, "scope", "Conformance inventory")
        value_change = text.replace(
            '  ASSAY_PUBLIC_MSRV: "1.89.0"\n',
            '  ASSAY_PUBLIC_MSRV: "9.99.0"\n',
            1,
        )
        self.assertNotEqual(value_change, text)
        assert_hard_run_command(
            value_change, "scope", "Conformance inventory")

    def test_scope_trusted_prefix_fails_closed(self):
        text = CI_YML.read_text(encoding="utf-8")
        target = "      - name: Conformance inventory\n"
        checkout = (
            "      - uses: actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09 # v5.1.0\n"
            "        with:\n"
            "          persist-credentials: false\n"
            "          fetch-depth: 0\n\n")
        setup = (
            "      - uses: actions/setup-python@ece7cb06caefa5fff74198d8649806c4678c61a1 # v6.3.0\n"
            "        with:\n"
            "          python-version: \"3.12\"\n\n")
        for label, mutated in trusted_prefix_mutations(
                text, target, checkout, setup):
            with self.subTest(label=label):
                self.assertNotEqual(mutated, text)
                with self.assertRaises(AssertionError):
                    assert_hard_run_command(
                        mutated, "scope", "Conformance inventory")

        target_step = named_step(text, "scope", "Conformance inventory")
        post_target = target_step + (
            "      - name: Harmless post-target control\n"
            "        run: echo harmless\n\n")
        assert_hard_run_command(
            text.replace(target_step, post_target, 1),
            "scope", "Conformance inventory")

    def test_conditional_scope_job_fails_the_inventory_guard(self):
        text = CI_YML.read_text(encoding="utf-8")
        mutated = text.replace(
            "  scope:\n",
            "  scope:\n    if: ${{ github.event_name == 'disabled' }}\n",
            1,
        )
        with self.assertRaises(AssertionError):
            assert_hard_run_command(
                mutated, "scope", "Conformance inventory")

    def test_softened_scope_job_fails_the_inventory_guard(self):
        text = CI_YML.read_text(encoding="utf-8")
        mutated = text.replace(
            "  scope:\n",
            "  scope:\n    continue-on-error: true\n",
            1,
        )
        with self.assertRaises(AssertionError):
            assert_hard_run_command(
                mutated, "scope", "Conformance inventory")

    def test_wider_indented_conditional_scope_job_fails(self):
        text = CI_YML.read_text(encoding="utf-8")
        job = named_job(text, "scope")
        lines = job.splitlines(keepends=True)
        widened = (
            lines[0]
            + "      if: ${{ github.event_name == 'disabled' }}\n"
            + "".join("  " + line if line.strip() else line for line in lines[1:])
        )
        with self.assertRaises(AssertionError):
            assert_hard_run_command(
                text.replace(job, widened, 1),
                "scope",
                "Conformance inventory",
            )
        with self.assertRaises(AssertionError):
            assert_hard_run_command(
                text.replace("    timeout-minutes: 10\n", "", 1),
                "scope",
                "Conformance inventory",
            )

    def test_custom_scope_shell_fails_the_inventory_guard(self):
        text = CI_YML.read_text(encoding="utf-8")
        step = named_step(text, "scope", "Conformance inventory")
        mutated_step = step.replace(
            "        shell: bash\n",
            "        shell: bash -c 'true' -- {0}\n",
            1,
        )
        self.assertNotEqual(mutated_step, step)
        with self.assertRaises(AssertionError):
            assert_hard_run_command(
                text.replace(step, mutated_step, 1),
                "scope",
                "Conformance inventory",
            )

    def test_quoted_scope_job_and_inventory_step_keys_fail(self):
        text = CI_YML.read_text(encoding="utf-8")
        job = "  scope:\n"
        step = "      - name: Conformance inventory\n"
        mutations = (
            ("job double if", job,
             job + "    \"if\": ${{ github.event_name == 'disabled' }}\n"),
            ("job single continue", job,
             job + "    'continue-on-error': true\n"),
            ("step double if", step,
             step + "        \"if\": ${{ github.event_name == 'disabled' }}\n"),
            ("step single continue", step,
             step + "        'continue-on-error': true\n"),
        )
        for label, needle, replacement in mutations:
            with self.subTest(label=label):
                with self.assertRaises(AssertionError):
                    assert_hard_run_command(
                        text.replace(needle, replacement, 1),
                        "scope", "Conformance inventory")

    def test_shell_neutralizers_fail_the_inventory_guard(self):
        text = CI_YML.read_text(encoding="utf-8")
        step = named_step(text, "scope", "Conformance inventory")
        mutations = (
            step.replace(
                "          set -euo pipefail\n",
                "          set -euo pipefail\n          set +o errexit\n",
                1,
            ).replace(PLAIN_RUN_ALL, PLAIN_RUN_ALL + "\n          true", 1),
            step.replace(
                "          set -euo pipefail\n",
                "          set -euo pipefail\n          python3() { :; }\n",
                1,
            ),
        )
        for index, mutated_step in enumerate(mutations):
            with self.subTest(index=index):
                with self.assertRaises(AssertionError):
                    assert_hard_run_command(
                        text.replace(step, mutated_step, 1),
                        "scope", "Conformance inventory")

    def test_relocated_run_all_command_fails_the_inventory_guard(self):
        text = CI_YML.read_text(encoding="utf-8")
        mutated = text.replace("          " + PLAIN_RUN_ALL, "          :", 1).replace(
            "      - name: Published-numbers projection contract\n",
            "      - name: Decorative completion-scope note\n"
            "        run: |\n"
            f"          {PLAIN_RUN_ALL}\n\n"
            "      - name: Published-numbers projection contract\n",
            1,
        )
        with self.assertRaises(AssertionError):
            assert_hard_run_command(
                mutated, "scope", "Conformance inventory")


class RegistryDoesNotRunAll(unittest.TestCase):
    def test_registry_py_does_not_invoke_or_neutralize_run_all(self):
        source = Path(registry.__file__).read_text(encoding="utf-8")
        self.assertNotIn("subprocess", source)
        self.assertNotIn("--require-complete", source)
        self.assertNotIn("returncode in (0, 3)", source)


if __name__ == "__main__":
    unittest.main(verbosity=2)
