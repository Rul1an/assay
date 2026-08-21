#!/usr/bin/env python3
"""Digest-addressed implementation registry: first RED is a tag-only image.

    python3 -W error::ResourceWarning conformance/tests/test_implementations.py

The validator and required CI must share one function. This file does not pull,
run, or network an image. Registration addresses bytes; it does not authenticate
a publisher or endorse a candidate.
"""

from __future__ import annotations

import json
import re
import sys
import tempfile
import unittest
from pathlib import Path
from urllib.parse import urlparse

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "conformance"))

try:
    import implementations
except ImportError:  # expected on the first RED before the loader exists
    implementations = None

CI_YML = REPO / ".github/workflows/ci.yml"
INVENTORY_STEP_RE = re.compile(r"(?ms)^      - name: Conformance inventory\n(?:        .+\n)+")
JOB_KEY_RE = re.compile(r"^  [A-Za-z0-9_][A-Za-z0-9_-]*:")
VALIDATOR_CALL = "python3 conformance/implementations.py"
TAG_ONLY_IMAGE = "ghcr.io/example/checker:latest"
DIGEST_IMAGE = (
    "ghcr.io/example/checker@sha256:"
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
)


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
        "authorship": "Authored-By: human",
    }
    row.update(overrides)
    return row


def _doc(rows):
    return {"schema": "assay.conformance.implementations.v0", "implementations": rows}


class TagOnlyImageIsTheFirstRed(unittest.TestCase):
    """Issue #198: tag-only image must fail the validator and the CI contract."""

    def test_tag_only_image_is_rejected_by_the_validator(self):
        self.assertIsNotNone(
            implementations,
            "stdlib validator conformance/implementations.py is missing",
        )
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "implementations.json"
            path.write_text(
                json.dumps(_doc([_valid_row(image=TAG_ONLY_IMAGE)])),
                encoding="utf-8",
            )
            with self.assertRaises(implementations.ImplementationRegistryError) as ctx:
                implementations.load_implementations(path)
        message = str(ctx.exception).lower()
        self.assertIn("image", message)
        self.assertTrue(
            "digest" in message or "sha256" in message,
            ctx.exception,
        )

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
        self.assertIsNotNone(
            implementations,
            "stdlib validator conformance/implementations.py is missing",
        )
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
        loaded = implementations.load_implementations(path)
        self.assertEqual(loaded["schema"], implementations.SCHEMA)
        self.assertEqual(len(loaded["implementations"]), 1)
        self.assertEqual(loaded["implementations"][0]["id"], "example-checker")

    def test_shipped_registry_is_loadable(self):
        loaded = implementations.load_implementations()
        self.assertEqual(loaded["schema"], implementations.SCHEMA)
        self.assertIsInstance(loaded["implementations"], list)

    def test_duplicate_id_is_rejected(self):
        path = self._write(_doc([_valid_row(), _valid_row(name="Other")]))
        with self.assertRaises(implementations.ImplementationRegistryError) as ctx:
            implementations.load_implementations(path)
        self.assertIn("duplicate", str(ctx.exception).lower())

    def test_unknown_suite_is_rejected(self):
        path = self._write(_doc([_valid_row(suite="privileged-mcp-action-v1")]))
        with self.assertRaises(implementations.ImplementationRegistryError) as ctx:
            implementations.load_implementations(path)
        self.assertIn("suite", str(ctx.exception).lower())

    def test_unknown_reproduction_mode_is_rejected(self):
        path = self._write(_doc([_valid_row(reproduction_mode="from_expected_values")]))
        with self.assertRaises(implementations.ImplementationRegistryError) as ctx:
            implementations.load_implementations(path)
        self.assertIn("reproduction_mode", str(ctx.exception).lower())

    def test_missing_source_commit_is_rejected(self):
        row = _valid_row()
        del row["commit"]
        path = self._write(_doc([row]))
        with self.assertRaises(implementations.ImplementationRegistryError) as ctx:
            implementations.load_implementations(path)
        self.assertIn("commit", str(ctx.exception).lower())

    def test_short_source_commit_is_rejected(self):
        path = self._write(_doc([_valid_row(commit="0123456789abcdef")]))
        with self.assertRaises(implementations.ImplementationRegistryError) as ctx:
            implementations.load_implementations(path)
        self.assertIn("commit", str(ctx.exception).lower())

    def test_missing_authorship_disclosure_is_rejected(self):
        row = _valid_row()
        del row["authorship"]
        path = self._write(_doc([row]))
        with self.assertRaises(implementations.ImplementationRegistryError) as ctx:
            implementations.load_implementations(path)
        self.assertIn("authorship", str(ctx.exception).lower())

    def test_unknown_field_is_rejected(self):
        row = _valid_row(tier="T0")
        path = self._write(_doc([row]))
        with self.assertRaises(implementations.ImplementationRegistryError) as ctx:
            implementations.load_implementations(path)
        self.assertIn("unknown", str(ctx.exception).lower())

    def test_relative_source_url_is_rejected(self):
        path = self._write(_doc([_valid_row(source="github.com/example/checker")]))
        with self.assertRaises(implementations.ImplementationRegistryError) as ctx:
            implementations.load_implementations(path)
        parsed = urlparse("github.com/example/checker")
        self.assertFalse(parsed.scheme)
        self.assertIn("source", str(ctx.exception).lower())

    def test_tag_plus_digest_image_is_rejected(self):
        image = (
            "ghcr.io/example/checker:latest@sha256:"
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        )
        path = self._write(_doc([_valid_row(image=image)]))
        with self.assertRaises(implementations.ImplementationRegistryError):
            implementations.load_implementations(path)


class OneRuleNoNetwork(unittest.TestCase):
    def test_validator_does_not_pull_run_or_network(self):
        self.assertIsNotNone(implementations)
        source = Path(implementations.__file__).read_text(encoding="utf-8")
        self.assertNotIn("subprocess", source)
        self.assertNotIn("urllib.request", source)
        self.assertNotIn("docker", source.lower())
        self.assertNotIn("http.client", source)

    def test_main_uses_the_same_loader(self):
        self.assertIsNotNone(implementations)
        source = Path(implementations.__file__).read_text(encoding="utf-8")
        self.assertIn("load_implementations()", source)

    def test_schema_forbids_unknown_fields(self):
        schema_path = REPO / "conformance/implementations.schema.json"
        schema = json.loads(schema_path.read_text(encoding="utf-8"))
        self.assertIs(schema["additionalProperties"], False)
        row = schema["properties"]["implementations"]["items"]
        self.assertIs(row["additionalProperties"], False)


if __name__ == "__main__":
    unittest.main(verbosity=2)
