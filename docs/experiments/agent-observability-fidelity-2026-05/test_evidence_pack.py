"""Tests for the agent-observability evidence-pack prototype."""

from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from datetime import datetime
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent
PACK_PATH = ROOT / "evidence_pack.py"

spec = importlib.util.spec_from_file_location("evidence_pack", PACK_PATH)
assert spec is not None and spec.loader is not None
evidence_pack = importlib.util.module_from_spec(spec)
spec.loader.exec_module(evidence_pack)


def load_schema(name: str) -> dict[str, Any]:
    return json.loads((ROOT / "schema" / name).read_text(encoding="utf-8"))


def resolve_ref(schema: dict[str, Any], ref: str) -> dict[str, Any]:
    if not ref.startswith("#/$defs/"):
        raise AssertionError(f"unsupported $ref: {ref}")
    target: Any = schema
    for part in ref.removeprefix("#/").split("/"):
        target = target[part]
    if not isinstance(target, dict):
        raise AssertionError(f"$ref did not resolve to object: {ref}")
    return target


def condition_matches(
    payload: Any,
    node: dict[str, Any],
    *,
    root: dict[str, Any],
) -> bool:
    if "$ref" in node:
        return condition_matches(payload, resolve_ref(root, node["$ref"]), root=root)

    if "const" in node and payload != node["const"]:
        return False
    if "enum" in node and payload not in node["enum"]:
        return False

    expected_type = node.get("type")
    if expected_type is not None:
        types = expected_type if isinstance(expected_type, list) else [expected_type]
        type_ok = False
        for typ in types:
            if typ == "object":
                type_ok = isinstance(payload, dict)
            elif typ == "array":
                type_ok = isinstance(payload, list)
            elif typ == "string":
                type_ok = isinstance(payload, str)
            elif typ == "integer":
                type_ok = isinstance(payload, int) and not isinstance(payload, bool)
            elif typ == "boolean":
                type_ok = isinstance(payload, bool)
            elif typ == "null":
                type_ok = payload is None
            else:
                raise AssertionError(f"unsupported type {typ!r}")
            if type_ok:
                break
        if not type_ok:
            return False

    if isinstance(payload, dict):
        for key in node.get("required", []):
            if key not in payload:
                return False
        for key, child in node.get("properties", {}).items():
            if key in payload and not condition_matches(
                payload[key],
                child,
                root=root,
            ):
                return False

    return True


def assert_matches_schema(
    test: unittest.TestCase,
    payload: Any,
    node: dict[str, Any],
    *,
    root: dict[str, Any],
    path: str = "$",
) -> None:
    if "$ref" in node:
        return assert_matches_schema(
            test, payload, resolve_ref(root, node["$ref"]), root=root, path=path
        )

    expected_type = node.get("type")
    if expected_type is not None:
        types = expected_type if isinstance(expected_type, list) else [expected_type]
        if "null" in types and payload is None:
            return
        type_ok = False
        for typ in types:
            if typ == "object":
                type_ok = isinstance(payload, dict)
            elif typ == "array":
                type_ok = isinstance(payload, list)
            elif typ == "string":
                type_ok = isinstance(payload, str)
            elif typ == "integer":
                type_ok = isinstance(payload, int) and not isinstance(payload, bool)
            elif typ == "boolean":
                type_ok = isinstance(payload, bool)
            elif typ == "null":
                type_ok = payload is None
            else:
                raise AssertionError(f"{path}: unsupported type {typ!r}")
            if type_ok:
                break
        test.assertTrue(type_ok, f"{path}: expected {types}, got {type(payload)}")

    if "const" in node:
        test.assertEqual(payload, node["const"], path)
    if "enum" in node:
        test.assertIn(payload, node["enum"], path)
    if "pattern" in node and isinstance(payload, str):
        test.assertRegex(payload, node["pattern"], path)
    if "minLength" in node and isinstance(payload, str):
        test.assertGreaterEqual(len(payload), node["minLength"], path)
    if node.get("format") == "date-time":
        datetime.fromisoformat(payload.replace("Z", "+00:00"))
    if "minimum" in node and payload is not None:
        test.assertGreaterEqual(payload, node["minimum"], path)

    if isinstance(payload, list):
        if "minItems" in node:
            test.assertGreaterEqual(len(payload), node["minItems"], path)
        item_node = node.get("items")
        if item_node is not None:
            for index, item in enumerate(payload):
                assert_matches_schema(
                    test, item, item_node, root=root, path=f"{path}[{index}]"
                )

    if isinstance(payload, dict):
        required = node.get("required", [])
        for key in required:
            test.assertIn(key, payload, f"{path}: missing {key}")
        properties = node.get("properties", {})
        if node.get("additionalProperties") is False:
            test.assertLessEqual(set(payload), set(properties), path)
        for key, value in payload.items():
            if key in properties:
                assert_matches_schema(
                    test, value, properties[key], root=root, path=f"{path}.{key}"
                )

    for index, child in enumerate(node.get("allOf", [])):
        if "if" in child and "then" in child:
            if condition_matches(payload, child["if"], root=root):
                assert_matches_schema(
                    test,
                    payload,
                    child["then"],
                    root=root,
                    path=f"{path}.allOf[{index}].then",
                )
        else:
            assert_matches_schema(
                test,
                payload,
                child,
                root=root,
                path=f"{path}.allOf[{index}]",
            )


def write_inputs(root: Path, *, ringbuf_drops: int = 0) -> tuple[Path, Path, Path]:
    runner_archive = root / "runner-archive.tar.gz"
    trace_json = root / "trace.json"
    health = root / "observation-health.json"
    runner_archive.write_bytes(b"runner archive bytes\n")
    trace_json.write_text(
        json.dumps({"resourceSpans": [{"scopeSpans": [{"spans": []}]}]}) + "\n",
        encoding="utf-8",
    )
    health.write_text(
        json.dumps(
            {
                "kernel_layer": "complete",
                "ringbuf_drops": ringbuf_drops,
                "cgroup_correlation": "clean",
            }
        )
        + "\n",
        encoding="utf-8",
    )
    return runner_archive, trace_json, health


class EvidencePackTests(unittest.TestCase):
    def assert_valid_pack(self, pack_dir: Path) -> dict[str, Any]:
        manifest = json.loads((pack_dir / "manifest.json").read_text(encoding="utf-8"))
        redaction = json.loads(
            (pack_dir / "redaction-manifest.json").read_text(encoding="utf-8")
        )
        pack_schema = load_schema("evidence-pack-v0.schema.json")
        redaction_schema = load_schema("redaction-manifest-v0.schema.json")
        assert_matches_schema(self, manifest, pack_schema, root=pack_schema)
        assert_matches_schema(
            self, redaction, redaction_schema, root=redaction_schema
        )
        return manifest

    def test_create_pack_with_trace_and_clean_health(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive, trace, health = write_inputs(root)
            out = root / "pack"

            evidence_pack.build_pack(
                out_dir=out,
                scenario_id="matched-safe-read",
                claim_class="positive_join",
                claim_summary="Trace and archive agree for safe.txt.",
                runner_archive=archive,
                trace_json=trace,
                observation_health=health,
                created_at="2026-05-28T08:00:00Z",
                redaction_policy="none",
            )

            manifest = self.assert_valid_pack(out)
            self.assertEqual(
                manifest["schema"],
                "assay.experiment.agent_observability_fidelity.evidence_pack.v0",
            )
            self.assertEqual(manifest["scenario_id"], "matched-safe-read")
            self.assertEqual(manifest["claim_class"], "positive_join")
            self.assertEqual(manifest["observation_health_status"], "clean")
            self.assertFalse(manifest["redaction"]["redaction_applied"])
            self.assertIn(
                "does_not_strengthen_underlying_claims", manifest["non_claims"]
            )
            roles = {row["role"]: row for row in manifest["artifacts"]}
            self.assertEqual(
                roles["runner_archive"]["path"], "artifacts/runner-archive.tar.gz"
            )
            self.assertEqual(roles["trace_json"]["path"], "artifacts/trace.json")
            self.assertEqual(
                roles["observation_health"]["path"],
                "artifacts/observation-health.json",
            )
            self.assertEqual(
                roles["redaction_manifest"]["path"],
                "redaction-manifest.json",
            )
            self.assertEqual(roles["summary_markdown"]["path"], "summary.md")
            for row in manifest["artifacts"]:
                self.assertRegex(row["sha256"], r"^sha256:[0-9a-f]{64}$")
                self.assertFalse(Path(row["path"]).is_absolute())
                self.assertTrue((out / row["path"]).exists())

            summary = (out / "summary.md").read_text(encoding="utf-8")
            self.assertIn("Trace and archive agree for safe.txt.", summary)
            self.assertIn("does_not_promote_evidence_pack_to_product_api", summary)

    def test_pack_without_trace_keeps_trace_input_null(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive, _trace, health = write_inputs(root)
            out = root / "pack"

            evidence_pack.build_pack(
                out_dir=out,
                scenario_id="archive-only-diagnostic",
                claim_class="diagnostic",
                claim_summary="Archive-only health diagnostic.",
                runner_archive=archive,
                trace_json=None,
                observation_health=health,
                created_at="2026-05-28T08:00:00Z",
                redaction_policy="none",
            )

            manifest = self.assert_valid_pack(out)
            self.assertNotIn(
                "trace_json", {row["role"] for row in manifest["artifacts"]}
            )
            self.assertIsNone(manifest["reproduction"]["inputs"]["trace_json"])

    def test_health_with_drops_is_inconclusive(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive, trace, health = write_inputs(root, ringbuf_drops=3)
            out = root / "pack"

            evidence_pack.build_pack(
                out_dir=out,
                scenario_id="drop-diagnostic",
                claim_class="diagnostic",
                claim_summary="Kernel drops make this diagnostic-only.",
                runner_archive=archive,
                trace_json=trace,
                observation_health=health,
                created_at="2026-05-28T08:00:00Z",
                redaction_policy="none",
            )

            manifest = self.assert_valid_pack(out)
            self.assertEqual(manifest["observation_health_status"], "inconclusive")
            self.assertEqual(manifest["observation_health"]["ringbuf_drops"], 3)

    def test_missing_input_fails_before_writing_pack(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive, _trace, health = write_inputs(root)
            missing_trace = root / "missing-trace.json"

            with self.assertRaises(FileNotFoundError):
                evidence_pack.main(
                    [
                        "create",
                        "--out-dir",
                        str(root / "pack"),
                        "--scenario-id",
                        "missing-input",
                        "--claim-summary",
                        "missing input should fail",
                        "--runner-archive",
                        str(archive),
                        "--trace-json",
                        str(missing_trace),
                        "--observation-health",
                        str(health),
                    ]
                )

            self.assertFalse((root / "pack").exists())

    def test_existing_nonempty_output_directory_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive, trace, health = write_inputs(root)
            out = root / "pack"
            out.mkdir()
            (out / "stale.txt").write_text("stale\n", encoding="utf-8")

            with self.assertRaises(FileExistsError):
                evidence_pack.build_pack(
                    out_dir=out,
                    scenario_id="stale-output",
                    claim_class="diagnostic",
                    claim_summary="stale output should fail",
                    runner_archive=archive,
                    trace_json=trace,
                    observation_health=health,
                    created_at="2026-05-28T08:00:00Z",
                    redaction_policy="none",
                )


if __name__ == "__main__":
    unittest.main()
