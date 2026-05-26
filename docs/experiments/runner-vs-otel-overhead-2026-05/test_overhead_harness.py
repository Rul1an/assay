"""Tests for the runner-vs-OTel overhead Slice 1 harness."""

from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from datetime import datetime
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent
HARNESS_PATH = ROOT / "overhead_harness.py"

spec = importlib.util.spec_from_file_location("overhead_harness", HARNESS_PATH)
assert spec is not None and spec.loader is not None
overhead_harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(overhead_harness)


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

    if "oneOf" in node:
        errors = []
        for option in node["oneOf"]:
            try:
                assert_matches_schema(test, payload, option, root=root, path=path)
                return
            except AssertionError as exc:
                errors.append(str(exc))
        raise AssertionError(f"{path}: no oneOf option matched: {errors}")

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
            elif typ == "number":
                type_ok = (
                    isinstance(payload, (int, float)) and not isinstance(payload, bool)
                )
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
        value = payload.replace("Z", "+00:00")
        datetime.fromisoformat(value)
    if "minimum" in node and payload is not None:
        test.assertGreaterEqual(payload, node["minimum"], path)

    if isinstance(payload, dict):
        required = node.get("required", [])
        for key in required:
            test.assertIn(key, payload, f"{path}: missing {key}")
        properties = node.get("properties", {})
        if node.get("additionalProperties") is False:
            test.assertLessEqual(set(payload), set(properties), path)
        additional = node.get("additionalProperties")
        for key, value in payload.items():
            if key in properties:
                assert_matches_schema(
                    test, value, properties[key], root=root, path=f"{path}.{key}"
                )
            elif isinstance(additional, dict):
                assert_matches_schema(
                    test, value, additional, root=root, path=f"{path}.{key}"
                )


def write_stub_workload(root: Path) -> None:
    (root / "dist").mkdir(parents=True)
    (root / "package.json").write_text(
        json.dumps({"name": "overhead-stub-workload", "version": "0.0.0"}),
        encoding="utf-8",
    )
    (root / "dist" / "workload.js").write_text(
        """
const fs = require("fs");
const path = require("path");
const args = process.argv.slice(2);
const get = (name) => {
  const idx = args.indexOf(`--${name}`);
  return idx >= 0 ? args[idx + 1] : undefined;
};
const traceOut = get("trace-out");
const workDir = get("work-dir");
fs.mkdirSync(path.dirname(traceOut), { recursive: true });
fs.mkdirSync(workDir, { recursive: true });
fs.writeFileSync(traceOut, JSON.stringify({ resourceSpans: [] }) + "\\n");
""".strip()
        + "\n",
        encoding="utf-8",
    )


class OverheadHarnessTests(unittest.TestCase):
    def sample(self, **overrides: Any) -> dict[str, Any]:
        payload = {
            "schema": overhead_harness.SAMPLE_SCHEMA,
            "experiment": overhead_harness.EXPERIMENT,
            "arm": "arm-b-otel",
            "iteration": 1,
            "host": "devhost",
            "host_class": "darwin-arm64-23.0.0",
            "assay_commit": "abcdef1",
            "started_at": "2026-05-26T00:00:00Z",
            "tool_versions": {
                "python": "3.12.0",
                "node": "v22.16.0",
                "npm": "10.0.0",
                "hyperfine": None,
                "time": "python-time.perf_counter",
                "workload_package": "0.0.0",
            },
            "wall_clock_ms": 12.5,
            "peak_rss_bytes": None,
            "exit_code": 0,
            "health": None,
            "artifact_bytes": {
                "trace_json": 123,
                "archive_targz": None,
                "archive_extracted": None,
            },
        }
        payload.update(overrides)
        return payload

    def test_sample_schema_accepts_arm_b_sample(self) -> None:
        schema = load_schema("overhead-sample-v0.schema.json")
        assert_matches_schema(self, self.sample(), schema, root=schema)

    def test_sample_schema_requires_extracted_size_key(self) -> None:
        schema = load_schema("overhead-sample-v0.schema.json")
        sample = self.sample()
        del sample["artifact_bytes"]["archive_extracted"]
        with self.assertRaises(AssertionError):
            assert_matches_schema(self, sample, schema, root=schema)

    def test_summary_schema_accepts_summary(self) -> None:
        samples = [
            self.sample(iteration=1, wall_clock_ms=10.0),
            self.sample(
                iteration=2,
                wall_clock_ms=20.0,
                artifact_bytes={
                    "trace_json": 200,
                    "archive_targz": None,
                    "archive_extracted": None,
                },
            ),
        ]
        summary = overhead_harness.summarize(samples, delegated_workflow_url=None)
        schema = load_schema("overhead-summary-v0.schema.json")
        assert_matches_schema(self, summary, schema, root=schema)
        self.assertEqual(summary["valid_samples"], 2)
        self.assertEqual(summary["wall_clock_ms"]["median"], 15.0)
        self.assertEqual(summary["artifact_bytes"]["trace_json_median"], 161.5)

    def test_summary_schema_requires_provenance(self) -> None:
        summary = overhead_harness.summarize(
            [self.sample()],
            delegated_workflow_url=None,
        )
        del summary["host_class"]
        schema = load_schema("overhead-summary-v0.schema.json")
        with self.assertRaises(AssertionError):
            assert_matches_schema(self, summary, schema, root=schema)

    def test_bmf_export_is_metric_keyed_value_objects(self) -> None:
        summary = overhead_harness.summarize(
            [self.sample()],
            delegated_workflow_url=None,
        )
        bmf = overhead_harness.bmf_export(summary)
        self.assertIn("runner_vs_otel.arm_b.wall_clock_ms.median", bmf)
        self.assertTrue(all(set(value) == {"value"} for value in bmf.values()))

    def test_host_class_is_schema_safe(self) -> None:
        self.assertRegex(overhead_harness.host_class(), r"^[A-Za-z0-9_.-]+$")

    def test_percentile_uses_nearest_rank(self) -> None:
        self.assertEqual(overhead_harness.percentile([1, 2, 3, 4, 5], 95), 5)
        self.assertEqual(overhead_harness.percentile([1, 2, 3, 4, 5], 50), 3)

    def test_negative_returncode_is_schema_safe(self) -> None:
        self.assertEqual(overhead_harness.normalized_exit_code(-15), 143)
        self.assertEqual(overhead_harness.normalized_exit_code(2), 2)

    def test_tool_versions_schema_rejects_typo_keys(self) -> None:
        schema = load_schema("overhead-sample-v0.schema.json")
        sample = self.sample()
        sample["tool_versions"]["hyprfine"] = None
        with self.assertRaises(AssertionError):
            assert_matches_schema(self, sample, schema, root=schema)

    def test_harness_emits_twenty_valid_stub_samples(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            workload = root / "workload"
            out_dir = root / "overhead"
            write_stub_workload(workload)

            status = overhead_harness.main(
                [
                    "--iterations",
                    "20",
                    "--skip-build",
                    "--clean",
                    "--timeout-seconds",
                    "10",
                    "--workload-dir",
                    str(workload),
                    "--out-dir",
                    str(out_dir),
                ]
            )
            self.assertEqual(status, 0)

            samples = [
                json.loads(line)
                for line in (out_dir / "arm-b-otel" / "samples.jsonl")
                .read_text(encoding="utf-8")
                .splitlines()
            ]
            summary = json.loads(
                (out_dir / "arm-b-otel" / "summary.json").read_text(
                    encoding="utf-8"
                )
            )
            bmf = json.loads(
                (out_dir / "artifacts" / "bmf.json").read_text(encoding="utf-8")
            )

            sample_schema = load_schema("overhead-sample-v0.schema.json")
            summary_schema = load_schema("overhead-summary-v0.schema.json")
            self.assertEqual(len(samples), 20)
            for sample in samples:
                assert_matches_schema(self, sample, sample_schema, root=sample_schema)
            assert_matches_schema(self, summary, summary_schema, root=summary_schema)
            self.assertEqual(summary["valid_samples"], 20)
            self.assertEqual(summary["discarded_samples"], 0)
            self.assertTrue(bmf)


if __name__ == "__main__":
    unittest.main()
