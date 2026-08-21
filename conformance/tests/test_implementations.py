#!/usr/bin/env python3
"""Digest-addressed implementation registry: first RED is a tag-only image.

    python3 -W error::ResourceWarning conformance/tests/test_implementations.py

The validator and required CI must share one function. This file does not pull,
run, or network an image. Registration addresses bytes; it does not authenticate
a publisher or endorse a candidate.
"""

from __future__ import annotations

import ast
import json
import os
import re
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock
from urllib.parse import urlparse

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "conformance"))
sys.path.insert(0, str(REPO / "conformance/adequacy"))

try:
    import implementations
except ModuleNotFoundError as exc:
    if getattr(exc, "name", None) != "implementations":
        raise
    implementations = None

import published_rows  # noqa: E402

CI_YML = REPO / ".github/workflows/ci.yml"
INVENTORY_STEP_RE = re.compile(r"(?ms)^      - name: Conformance inventory\n(?:        .+\n)+")
JOB_KEY_RE = re.compile(r"^  [A-Za-z0-9_][A-Za-z0-9_-]*:")
VALIDATOR_CALL = "python3 conformance/implementations.py"
TAG_ONLY_IMAGE = "ghcr.io/example/checker:latest"
DIGEST_IMAGE = (
    "ghcr.io/example/checker@sha256:"
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
)
HUMAN_AUTHORSHIP = {"kind": "human"}
ASSISTED_AUTHORSHIP = {
    "kind": "agent-assisted",
    "model": "claude-opus",
    "prompt_strategy": "spec-then-conformance",
}


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


def _valid_row(**overrides):
    row = {
        "id": "example-checker",
        "name": "Example Checker",
        "suite": "privileged-mcp-action-v0",
        "image": DIGEST_IMAGE,
        "source": "https://github.com/example/checker",
        "commit": "0123456789abcdef0123456789abcdef01234567",
        "language": "rust",
        "reproduction_mode": "blind_from_spec",
        "authorship": dict(HUMAN_AUTHORSHIP),
    }
    row.update(overrides)
    return row


def _doc(rows):
    return {"schema": "assay.conformance.implementations.v0", "implementations": rows}


def _require_module():
    if implementations is None:
        raise AssertionError("stdlib validator conformance/implementations.py is missing")
    return implementations


class ImportBoundary(unittest.TestCase):
    def test_unrelated_missing_module_is_not_treated_as_absent_registry(self):
        with self.assertRaises(ModuleNotFoundError) as ctx:
            try:
                raise ModuleNotFoundError(
                    "No module named 'not_stdlib'", name="not_stdlib"
                )
            except ModuleNotFoundError as exc:
                if getattr(exc, "name", None) != "implementations":
                    raise
        self.assertEqual(ctx.exception.name, "not_stdlib")


class TagOnlyImageIsTheFirstRed(unittest.TestCase):
    """Issue #198: tag-only image must fail the validator and the CI contract."""

    def test_tag_only_image_is_rejected_by_the_validator(self):
        module = _require_module()
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "implementations.json"
            path.write_text(
                json.dumps(_doc([_valid_row(image=TAG_ONLY_IMAGE)])),
                encoding="utf-8",
            )
            with self.assertRaises(module.ImplementationRegistryError) as ctx:
                module.load_implementations(path)
        message = str(ctx.exception).lower()
        self.assertIn("image", message)
        self.assertTrue("digest" in message or "sha256" in message, ctx.exception)

    def test_required_ci_invokes_the_same_validator(self):
        step = _inventory_step(CI_YML.read_text(encoding="utf-8"))
        self.assertIn(VALIDATOR_CALL, step)
        self.assertIn("conformance/tests/test_implementations.py", step)
        self.assertNotIn("continue-on-error", step)

    def test_removing_the_validator_callsite_fails_the_ci_contract(self):
        text = CI_YML.read_text(encoding="utf-8")
        mutated = text.replace(VALIDATOR_CALL + "\n", "")
        self.assertNotEqual(mutated, text)
        step = _inventory_step(mutated)
        with self.assertRaises(AssertionError):
            self.assertIn(VALIDATOR_CALL, step)


class PositiveFixtureAndHostileMatrix(unittest.TestCase):
    def setUp(self):
        self.module = _require_module()
        self._tmp = tempfile.TemporaryDirectory()
        self.raw = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()

    def _write(self, payload):
        path = self.raw / "implementations.json"
        path.write_text(
            payload if isinstance(payload, str) else json.dumps(payload),
            encoding="utf-8",
        )
        return path

    def test_minimal_digest_row_loads(self):
        path = self._write(_doc([_valid_row()]))
        loaded = self.module.load_implementations(path)
        self.assertEqual(loaded["schema"], self.module.SCHEMA)
        self.assertEqual(len(loaded["implementations"]), 1)
        row = loaded["implementations"][0]
        self.assertEqual(row["id"], "example-checker")
        self.assertEqual(row["language"], "rust")
        self.assertEqual(row["authorship"], HUMAN_AUTHORSHIP)

    def test_agent_assisted_requires_nonempty_model_and_prompt_strategy(self):
        path = self._write(_doc([_valid_row(authorship=dict(ASSISTED_AUTHORSHIP))]))
        loaded = self.module.load_implementations(path)
        self.assertEqual(loaded["implementations"][0]["authorship"], ASSISTED_AUTHORSHIP)
        for field in ("model", "prompt_strategy"):
            broken = dict(ASSISTED_AUTHORSHIP)
            broken[field] = ""
            path = self._write(_doc([_valid_row(id="broken-%s" % field, authorship=broken)]))
            with self.subTest(field=field), self.assertRaises(
                self.module.ImplementationRegistryError
            ) as ctx:
                self.module.load_implementations(path)
            self.assertIn(field, str(ctx.exception).lower())
        kind_only = self._write(_doc([_valid_row(id="kind-only", authorship={"kind": "agent-generated"})]))
        with self.assertRaises(self.module.ImplementationRegistryError) as ctx:
            self.module.load_implementations(kind_only)
        message = str(ctx.exception).lower()
        self.assertTrue("model" in message or "prompt_strategy" in message, ctx.exception)

    def test_human_authorship_forbids_model_and_prompt_strategy(self):
        for extra in (
            {"kind": "human", "model": "claude-opus"},
            {"kind": "human", "prompt_strategy": "spec-first"},
        ):
            path = self._write(_doc([_valid_row(id="human-extra", authorship=extra)]))
            with self.subTest(extra=extra), self.assertRaises(
                self.module.ImplementationRegistryError
            ) as ctx:
                self.module.load_implementations(path)
            self.assertIn("authorship", str(ctx.exception).lower())

    def test_whitespace_in_source_url_is_rejected(self):
        path = self._write(_doc([_valid_row(source="https://exa mple.com/a")]))
        with self.assertRaises(self.module.ImplementationRegistryError) as ctx:
            self.module.load_implementations(path)
        self.assertIn("source", str(ctx.exception).lower())

    def test_authorship_string_is_rejected(self):
        path = self._write(_doc([_valid_row(authorship="Authored-By: human")]))
        with self.assertRaises(self.module.ImplementationRegistryError) as ctx:
            self.module.load_implementations(path)
        self.assertIn("authorship", str(ctx.exception).lower())

    def test_shipped_registry_is_loadable(self):
        loaded = self.module.load_implementations()
        self.assertEqual(loaded["schema"], self.module.SCHEMA)
        self.assertIsInstance(loaded["implementations"], list)

    def test_duplicate_id_is_rejected(self):
        path = self._write(_doc([_valid_row(), _valid_row(name="Other")]))
        with self.assertRaises(self.module.ImplementationRegistryError) as ctx:
            self.module.load_implementations(path)
        self.assertIn("duplicate", str(ctx.exception).lower())

    def test_unknown_suite_is_rejected(self):
        path = self._write(_doc([_valid_row(suite="privileged-mcp-action-v1")]))
        with self.assertRaises(self.module.ImplementationRegistryError) as ctx:
            self.module.load_implementations(path)
        self.assertIn("suite", str(ctx.exception).lower())

    def test_unknown_reproduction_mode_is_rejected(self):
        path = self._write(_doc([_valid_row(reproduction_mode="from_expected_values")]))
        with self.assertRaises(self.module.ImplementationRegistryError) as ctx:
            self.module.load_implementations(path)
        self.assertIn("reproduction_mode", str(ctx.exception).lower())

    def test_missing_source_commit_is_rejected(self):
        row = _valid_row()
        del row["commit"]
        path = self._write(_doc([row]))
        with self.assertRaises(self.module.ImplementationRegistryError) as ctx:
            self.module.load_implementations(path)
        self.assertIn("commit", str(ctx.exception).lower())

    def test_short_source_commit_is_rejected(self):
        path = self._write(_doc([_valid_row(commit="0123456789abcdef")]))
        with self.assertRaises(self.module.ImplementationRegistryError) as ctx:
            self.module.load_implementations(path)
        self.assertIn("commit", str(ctx.exception).lower())

    def test_missing_authorship_disclosure_is_rejected(self):
        row = _valid_row()
        del row["authorship"]
        path = self._write(_doc([row]))
        with self.assertRaises(self.module.ImplementationRegistryError) as ctx:
            self.module.load_implementations(path)
        self.assertIn("authorship", str(ctx.exception).lower())

    def test_unknown_field_is_rejected(self):
        row = _valid_row(tier="T0")
        path = self._write(_doc([row]))
        with self.assertRaises(self.module.ImplementationRegistryError) as ctx:
            self.module.load_implementations(path)
        self.assertIn("unknown", str(ctx.exception).lower())

    def test_relative_source_url_is_rejected(self):
        path = self._write(_doc([_valid_row(source="github.com/example/checker")]))
        with self.assertRaises(self.module.ImplementationRegistryError) as ctx:
            self.module.load_implementations(path)
        parsed = urlparse("github.com/example/checker")
        self.assertFalse(parsed.scheme)
        self.assertIn("source", str(ctx.exception).lower())

    def test_tag_plus_digest_image_is_rejected(self):
        image = (
            "ghcr.io/example/checker:latest@sha256:"
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        )
        path = self._write(_doc([_valid_row(image=image)]))
        with self.assertRaises(self.module.ImplementationRegistryError):
            self.module.load_implementations(path)

    def test_double_slash_image_is_rejected(self):
        digest = DIGEST_IMAGE.split("@", 1)[1]
        path = self._write(_doc([_valid_row(image="ghcr.io/example//checker@" + digest)]))
        with self.assertRaises(self.module.ImplementationRegistryError) as ctx:
            self.module.load_implementations(path)
        self.assertIn("image", str(ctx.exception).lower())

    def test_long_adversarial_image_is_rejected_promptly(self):
        digest = DIGEST_IMAGE.split("@", 1)[1]
        image = "x" + ("/" * 80) + "y@" + digest
        started = time.monotonic()
        path = self._write(_doc([_valid_row(image=image)]))
        with self.assertRaises(self.module.ImplementationRegistryError) as ctx:
            self.module.load_implementations(path)
        self.assertLess(time.monotonic() - started, 0.5)
        self.assertIn("image", str(ctx.exception).lower())


class HostileRegistryInput(unittest.TestCase):
    def setUp(self):
        self.module = _require_module()
        self._tmp = tempfile.TemporaryDirectory()
        self.raw = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()

    def test_does_not_declare_a_second_byte_cap(self):
        self.assertFalse(
            hasattr(self.module, "MAX_REGISTRY_BYTES"),
            "reuse the shared regular-file cap; do not invent a second ceiling",
        )

    def test_loads_through_the_shared_regular_file_reader(self):
        path = self.raw / "implementations.json"
        path.write_text(json.dumps(_doc([])), encoding="utf-8")
        with mock.patch.object(
            published_rows, "read_regular_file", wraps=published_rows.read_regular_file
        ) as reader:
            self.module.load_implementations(path)
        self.assertTrue(reader.called, "load_implementations must reuse read_regular_file")

    def test_duplicate_json_keys_are_rejected(self):
        path = self.raw / "implementations.json"
        path.write_text(
            '{"schema":"assay.conformance.implementations.v0",'
            '"implementations":[],"schema":"other"}',
            encoding="utf-8",
        )
        with self.assertRaises((self.module.ImplementationRegistryError, ValueError)) as ctx:
            self.module.load_implementations(path)
        self.assertIn("duplicate", str(ctx.exception).lower())

    def test_nonfinite_json_numbers_are_rejected(self):
        row = json.dumps(_valid_row())
        for token in ("NaN", "Infinity", "-Infinity", "1e999"):
            mutated = row.replace('"Example Checker"', token, 1)
            path = self.raw / "implementations.json"
            path.write_text(
                '{"schema":"assay.conformance.implementations.v0",'
                '"implementations":[%s]}' % mutated,
                encoding="utf-8",
            )
            with self.subTest(token=token), self.assertRaises(
                (self.module.ImplementationRegistryError, ValueError)
            ) as ctx:
                self.module.load_implementations(path)
            message = str(ctx.exception).lower()
            self.assertTrue(
                any(part in message for part in ("finite", "nan", "infinity", "1e999")),
                ctx.exception,
            )

    @unittest.skipUnless(hasattr(os, "symlink"), "symlinks unavailable")
    def test_symlink_input_is_rejected_without_following(self):
        target = self.raw / "target.json"
        target.write_text(json.dumps(_doc([])), encoding="utf-8")
        link = self.raw / "implementations.json"
        link.symlink_to(target)
        with self.assertRaises((self.module.ImplementationRegistryError, ValueError)) as ctx:
            self.module.load_implementations(link)
        message = str(ctx.exception).lower()
        self.assertTrue("regular" in message or "symlink" in message, ctx.exception)
        self.assertNotIn("escape", message)

    def test_nonregular_input_is_rejected(self):
        with self.assertRaises((self.module.ImplementationRegistryError, ValueError)) as ctx:
            self.module.load_implementations(self.raw)
        self.assertIn("regular", str(ctx.exception).lower())

    @unittest.skipUnless(hasattr(os, "mkfifo"), "FIFOs unavailable")
    def test_fifo_is_rejected_without_waiting_for_a_writer(self):
        path = self.raw / "implementations.json"
        os.mkfifo(path)
        started = time.monotonic()
        with self.assertRaises((self.module.ImplementationRegistryError, ValueError)):
            self.module.load_implementations(path)
        self.assertLess(time.monotonic() - started, 1.0)


class OneRuleNoNetwork(unittest.TestCase):
    def test_validator_does_not_pull_run_or_network(self):
        module = _require_module()
        source = Path(module.__file__).read_text(encoding="utf-8")
        self.assertNotIn("subprocess", source)
        self.assertNotIn("urllib.request", source)
        self.assertNotIn("docker", source.lower())
        self.assertNotIn("http.client", source)

    def test_main_calls_load_implementations(self):
        module = _require_module()
        sentinel = {"schema": module.SCHEMA, "implementations": []}
        with mock.patch.object(module, "load_implementations", return_value=sentinel) as loader:
            self.assertEqual(module.main(), 0)
            loader.assert_called_once_with()
        tree = ast.parse(Path(module.__file__).read_text(encoding="utf-8"))
        calls = [
            node
            for node in ast.walk(tree)
            if isinstance(node, ast.FunctionDef) and node.name == "main"
            for child in ast.walk(node)
            if isinstance(child, ast.Call)
            and (
                (isinstance(child.func, ast.Name) and child.func.id == "load_implementations")
                or (
                    isinstance(child.func, ast.Attribute)
                    and child.func.attr == "load_implementations"
                )
            )
        ]
        self.assertTrue(calls, "main() AST must call load_implementations")

    def test_schema_matches_validator_vocabulary(self):
        module = _require_module()
        rendered = module.implementation_schema()
        on_disk = json.loads(
            (REPO / "conformance/implementations.schema.json").read_text(encoding="utf-8")
        )
        self.assertEqual(on_disk, rendered)
        items = rendered["properties"]["implementations"]["items"]
        self.assertEqual(frozenset(items["required"]), frozenset(module.ROW_FIELDS))
        self.assertEqual(frozenset(items["properties"]), frozenset(module.ROW_FIELDS))
        suite = items["properties"]["suite"]
        allowed = suite.get("enum") or [suite["const"]]
        self.assertEqual(frozenset(allowed), frozenset(module.ALLOWED_SUITES))
        modes = items["properties"]["reproduction_mode"]["enum"]
        self.assertEqual(frozenset(modes), frozenset(module.REPRODUCTION_MODES))
        authorship = items["properties"]["authorship"]
        branches = authorship["oneOf"]
        self.assertEqual(len(branches), 2)
        human = next(branch for branch in branches if branch["properties"]["kind"].get("const") == "human")
        agent = next(branch for branch in branches if "enum" in branch["properties"]["kind"])
        self.assertIs(human["additionalProperties"], False)
        self.assertEqual(frozenset(human["required"]), frozenset(module.HUMAN_AUTHORSHIP_FIELDS))
        self.assertEqual(frozenset(human["properties"]), frozenset(module.HUMAN_AUTHORSHIP_FIELDS))
        self.assertNotIn("model", human["properties"])
        self.assertNotIn("prompt_strategy", human["properties"])
        self.assertIs(agent["additionalProperties"], False)
        self.assertEqual(frozenset(agent["required"]), frozenset(module.AGENT_AUTHORSHIP_FIELDS))
        self.assertEqual(frozenset(agent["properties"]), frozenset(module.AGENT_AUTHORSHIP_FIELDS))
        self.assertEqual(frozenset(agent["properties"]["kind"]["enum"]), frozenset(module.AGENT_KINDS))
        self.assertEqual(items["properties"]["source"]["pattern"], module.SOURCE_PATTERN)
        self.assertEqual(items["properties"]["image"]["pattern"], module.IMAGE_PATTERN)


if __name__ == "__main__":
    unittest.main(verbosity=2)
