"""Tests for the runner-vs-OTel overhead Slice 1 harness."""

from __future__ import annotations

import importlib.util
import json
import platform
import subprocess
import tempfile
import tarfile
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


def write_fake_assay(root: Path) -> Path:
    fake = root / "fake-assay.py"
    fake.write_text(
        """
#!/usr/bin/env python3
import json
import sys
import tarfile
import tempfile
from pathlib import Path

args = sys.argv[1:]
output = Path(args[args.index("--output") + 1])
phase_timing = None
if "--phase-timing-log" in args:
    phase_timing = Path(args[args.index("--phase-timing-log") + 1])
trace = None
if "--" in args:
    child = args[args.index("--") + 1:]
    if "--trace-out" in child:
        trace = Path(child[child.index("--trace-out") + 1])
if trace is not None:
    trace.parent.mkdir(parents=True, exist_ok=True)
    trace.write_text(json.dumps({"resourceSpans": []}) + "\\n", encoding="utf-8")
if phase_timing is not None:
    phase_timing.parent.mkdir(parents=True, exist_ok=True)
    phase_timing.write_text(json.dumps({
        "schema": "assay.experiment.runner_phase_timing.v0",
        "run_id": "fake_run",
        "agent_shim": "openai-agents",
        "phases_ms": {
            "preflight_ms": 1.0,
            "cgroup_prepare_ms": 2.0,
            "monitor_attach_ms": 3.0,
            "child_spawn_ms": 4.0,
            "child_runtime_ms": 5.0,
            "event_flush_ms": 6.0,
            "archive_write_ms": 7.0
        },
        "exit_code": 0,
        "signal": None,
        "error": None
    }) + "\\n", encoding="utf-8")

output.parent.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp)
    (root / "observation-health.json").write_text(
        json.dumps({
            "kernel_layer": "complete",
            "ringbuf_drops": 0,
            "cgroup_correlation": "clean"
        }) + "\\n",
        encoding="utf-8",
    )
    with tarfile.open(output, "w:gz") as tar:
        tar.add(root / "observation-health.json", arcname="observation-health.json")
""".lstrip(),
        encoding="utf-8",
    )
    fake.chmod(0o755)
    return fake


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
                "rss_time": "/usr/bin/time -l",
                "workload_package": "0.0.0",
            },
            "wall_clock_ms": 12.5,
            "peak_rss_bytes": None,
            "exit_code": 0,
            "health": None,
            "phase_timings_ms": None,
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

    def test_sample_schema_accepts_phase_timings(self) -> None:
        schema = load_schema("overhead-sample-v0.schema.json")
        sample = self.sample(
            phase_timings_ms={
                "preflight_ms": 1.0,
                "child_runtime_ms": 42.0,
                "archive_write_ms": 7.0,
            }
        )
        assert_matches_schema(self, sample, schema, root=schema)

    def test_phase_timing_schema_accepts_side_log(self) -> None:
        schema = load_schema("runner-phase-timing-v0.schema.json")
        payload = {
            "schema": "assay.experiment.runner_phase_timing.v0",
            "run_id": "run_001",
            "agent_shim": "openai-agents",
            "phases_ms": {
                "preflight_ms": 1.0,
                "child_runtime_ms": 42.0,
                "archive_write_ms": 7.0,
            },
            "exit_code": 0,
            "signal": None,
            "error": None,
        }
        assert_matches_schema(self, payload, schema, root=schema)

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
            [
                self.sample(
                    peak_rss_bytes=1234,
                    phase_timings_ms={"child_runtime_ms": 42.0},
                )
            ],
            delegated_workflow_url=None,
        )
        bmf = overhead_harness.bmf_export(summary)
        self.assertIn("runner_vs_otel.arm_b_otel.wall_clock_ms.median", bmf)
        self.assertIn("runner_vs_otel.arm_b_otel.peak_rss_bytes.max", bmf)
        self.assertIn(
            "runner_vs_otel.arm_b_otel.phase_timings_ms.child_runtime_ms.median",
            bmf,
        )
        self.assertTrue(all(set(value) == {"value"} for value in bmf.values()))

    def test_summary_markdown_surfaces_core_review_fields(self) -> None:
        summary = overhead_harness.summarize(
            [
                self.sample(
                    iteration=1,
                    wall_clock_ms=10.0,
                    peak_rss_bytes=1000,
                    phase_timings_ms={"child_runtime_ms": 10.0},
                ),
                self.sample(
                    iteration=2,
                    wall_clock_ms=20.0,
                    peak_rss_bytes=2000,
                    phase_timings_ms={"child_runtime_ms": 20.0},
                ),
            ],
            delegated_workflow_url="https://github.com/Rul1an/assay/actions/runs/1",
        )
        markdown = overhead_harness.summary_markdown(
            summary,
            artifact_name="runner-otel-overhead-arm-c-1",
        )

        self.assertIn("## Runner-vs-OTel Overhead Summary", markdown)
        self.assertIn("| Valid samples | `2` |", markdown)
        self.assertIn("| Wall p99/median |", markdown)
        self.assertIn("(healthy)", markdown)
        self.assertIn("| Peak RSS max | `2,000 bytes` |", markdown)
        self.assertIn("### Phase Timings", markdown)
        self.assertIn("| `child_runtime_ms` | `15 ms` |", markdown)
        self.assertIn("runner-otel-overhead-arm-c-1", markdown)
        self.assertIn("Non-claim", markdown)

    def test_summary_markdown_handles_zero_valid_samples(self) -> None:
        summary = overhead_harness.summarize(
            [self.sample(exit_code=1)],
            delegated_workflow_url=None,
        )
        markdown = overhead_harness.summary_markdown(summary)

        self.assertIn("| Valid samples | `0` |", markdown)
        self.assertIn("| Wall median | `null` |", markdown)
        self.assertIn("| Wall p99/median | `null` (unknown) |", markdown)
        self.assertIn("| Peak RSS max | `null` |", markdown)

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

    def test_parse_gnu_time_peak_rss(self) -> None:
        stderr = "Maximum resident set size (kbytes): 42\n"
        self.assertEqual(
            overhead_harness.parse_peak_rss_bytes(stderr, system="linux"),
            42 * 1024,
        )

    def test_parse_darwin_time_peak_rss(self) -> None:
        stderr = "123456 maximum resident set size\n"
        self.assertEqual(
            overhead_harness.parse_peak_rss_bytes(stderr, system="darwin"),
            123456,
        )

    def test_parse_peak_rss_returns_none_for_missing_tool_output(self) -> None:
        self.assertIsNone(overhead_harness.parse_peak_rss_bytes("", system="linux"))

    def test_rss_preflight_rejects_missing_time_binary(self) -> None:
        error = overhead_harness.rss_time_preflight_error(
            system="linux",
            time_path=Path("/definitely-not-installed/time"),
        )
        self.assertIn("--measure-rss requires", error or "")

    def test_rss_preflight_rejects_unsupported_platform(self) -> None:
        error = overhead_harness.rss_time_preflight_error(
            system="plan9",
            time_path=Path("/usr/bin/time"),
        )
        self.assertIn("supports Linux and macOS only", error or "")

    def test_measure_rss_forces_stable_locale(self) -> None:
        seen: dict[str, Any] = {}

        def fake_run(
            command: list[str],
            **kwargs: Any,
        ) -> subprocess.CompletedProcess[str]:
            seen["command"] = command
            seen["env"] = kwargs.get("env")
            trace = command[command.index("--trace-out") + 1]
            Path(trace).parent.mkdir(parents=True, exist_ok=True)
            Path(trace).write_text('{"resourceSpans":[]}\n', encoding="utf-8")
            return subprocess.CompletedProcess(
                command,
                0,
                stdout="",
                stderr="123 maximum resident set size\n",
            )

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            workload = root / "workload"
            write_stub_workload(workload)
            original_run = overhead_harness.subprocess.run
            original_prefix = overhead_harness.rss_time_prefix
            original_parse = overhead_harness.parse_peak_rss_bytes
            try:
                overhead_harness.subprocess.run = fake_run  # type: ignore[assignment]
                overhead_harness.rss_time_prefix = lambda: ["/usr/bin/time", "-l"]  # type: ignore[assignment]
                overhead_harness.parse_peak_rss_bytes = (  # type: ignore[assignment]
                    lambda stderr, system=None: 123
                )
                sample = overhead_harness.one_sample(
                    arm_dir=root / "out",
                    arm=overhead_harness.ARM_B,
                    workload_dir=workload,
                    iteration=1,
                    commit="abcdef1",
                    versions=self.sample()["tool_versions"],
                    timeout_seconds=10,
                    measure_rss=True,
                )
            finally:
                overhead_harness.subprocess.run = original_run
                overhead_harness.rss_time_prefix = original_prefix
                overhead_harness.parse_peak_rss_bytes = original_parse

        self.assertEqual(sample["peak_rss_bytes"], 123)
        self.assertEqual(seen["env"]["LC_ALL"], "C")
        self.assertEqual(seen["env"]["LANG"], "C")

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
            summary_md = (out_dir / "arm-b-otel" / "summary.md").read_text(
                encoding="utf-8"
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
            self.assertIn("Runner-vs-OTel Overhead Summary", summary_md)
            self.assertIn("| Valid samples | `20` |", summary_md)

    @unittest.skipUnless(Path("/usr/bin/time").exists(), "/usr/bin/time not available")
    def test_harness_can_emit_rss_sample(self) -> None:
        if platform.system().lower() not in {"darwin", "linux"}:
            self.skipTest("RSS parser only supports Darwin and Linux")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            workload = root / "workload"
            out_dir = root / "overhead"
            write_stub_workload(workload)

            status = overhead_harness.main(
                [
                    "--iterations",
                    "1",
                    "--skip-build",
                    "--clean",
                    "--measure-rss",
                    "--timeout-seconds",
                    "10",
                    "--workload-dir",
                    str(workload),
                    "--out-dir",
                    str(out_dir),
                ]
            )
            self.assertEqual(status, 0)

            sample = json.loads(
                (out_dir / "arm-b-otel" / "samples.jsonl")
                .read_text(encoding="utf-8")
                .splitlines()[0]
            )
            summary = json.loads(
                (out_dir / "arm-b-otel" / "summary.json").read_text(
                    encoding="utf-8"
                )
            )
            rss_sizes = json.loads(
                (out_dir / "artifacts" / "rss-sizes.json").read_text(
                    encoding="utf-8"
                )
            )
            self.assertIsInstance(sample["peak_rss_bytes"], int)
            self.assertGreater(sample["peak_rss_bytes"], 0)
            self.assertEqual(summary["peak_rss_bytes"]["max"], sample["peak_rss_bytes"])
            self.assertEqual(rss_sizes["peak_rss_bytes"], [sample["peak_rss_bytes"]])

    def test_arm_c_sample_extracts_archive_health_and_sizes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            workload = root / "workload"
            out_dir = root / "overhead"
            write_stub_workload(workload)
            fake_assay = write_fake_assay(root)
            fake_ebpf = root / "assay-ebpf.o"
            fake_ebpf.write_bytes(b"fake ebpf")

            status = overhead_harness.main(
                [
                    "--arm",
                    "arm-c-dual-capture",
                    "--iterations",
                    "1",
                    "--skip-build",
                    "--clean",
                    "--timeout-seconds",
                    "10",
                    "--workload-dir",
                    str(workload),
                    "--out-dir",
                    str(out_dir),
                    "--assay-bin",
                    str(fake_assay),
                    "--ebpf-obj",
                    str(fake_ebpf),
                ]
            )
            self.assertEqual(status, 0)

            sample = json.loads(
                (out_dir / "arm-c-dual-capture" / "samples.jsonl")
                .read_text(encoding="utf-8")
                .splitlines()[0]
            )
            summary = json.loads(
                (out_dir / "arm-c-dual-capture" / "summary.json").read_text(
                    encoding="utf-8"
                )
            )
            archive_sizes = json.loads(
                (out_dir / "artifacts" / "archive-sizes.json").read_text(
                    encoding="utf-8"
                )
            )

            self.assertEqual(sample["health"]["kernel_layer"], "complete")
            self.assertEqual(sample["health"]["ringbuf_drops"], 0)
            self.assertEqual(sample["health"]["cgroup_correlation"], "clean")
            self.assertEqual(sample["phase_timings_ms"]["child_runtime_ms"], 5.0)
            self.assertGreater(sample["artifact_bytes"]["archive_targz"], 0)
            self.assertGreater(sample["artifact_bytes"]["archive_extracted"], 0)
            self.assertEqual(summary["valid_samples"], 1)
            self.assertEqual(
                summary["phase_timings_ms"]["child_runtime_ms"]["median"],
                5.0,
            )
            self.assertEqual(archive_sizes["arm"], "arm-c-dual-capture")
            self.assertEqual(len(archive_sizes["archive_targz_bytes"]), 1)

    def test_arm_a_sample_extracts_archive_without_trace(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            workload = root / "workload"
            out_dir = root / "overhead"
            fixture_agent = root / "fixture-agent.js"
            write_stub_workload(workload)
            fixture_agent.write_text("console.log('fake fixture')\n", encoding="utf-8")
            fake_assay = write_fake_assay(root)
            fake_ebpf = root / "assay-ebpf.o"
            fake_ebpf.write_bytes(b"fake ebpf")

            status = overhead_harness.main(
                [
                    "--arm",
                    "arm-a-runner-only",
                    "--iterations",
                    "1",
                    "--skip-build",
                    "--clean",
                    "--timeout-seconds",
                    "10",
                    "--workload-dir",
                    str(workload),
                    "--out-dir",
                    str(out_dir),
                    "--assay-bin",
                    str(fake_assay),
                    "--ebpf-obj",
                    str(fake_ebpf),
                    "--runner-fixture-agent",
                    str(fixture_agent),
                ]
            )
            self.assertEqual(status, 0)

            sample = json.loads(
                (out_dir / "arm-a-runner-only" / "samples.jsonl")
                .read_text(encoding="utf-8")
                .splitlines()[0]
            )
            summary = json.loads(
                (out_dir / "arm-a-runner-only" / "summary.json").read_text(
                    encoding="utf-8"
                )
            )
            bmf = json.loads(
                (out_dir / "artifacts" / "bmf.json").read_text(encoding="utf-8")
            )

            self.assertEqual(sample["arm"], "arm-a-runner-only")
            self.assertEqual(sample["artifact_bytes"]["trace_json"], None)
            self.assertEqual(sample["phase_timings_ms"]["archive_write_ms"], 7.0)
            self.assertGreater(sample["artifact_bytes"]["archive_targz"], 0)
            self.assertGreater(sample["artifact_bytes"]["archive_extracted"], 0)
            self.assertEqual(summary["valid_samples"], 1)
            self.assertEqual(
                summary["phase_timings_ms"]["archive_write_ms"]["median"],
                7.0,
            )
            self.assertIn(
                "runner_vs_otel.arm_a_runner_only.wall_clock_ms.median",
                bmf,
            )
            self.assertIn(
                "runner_vs_otel.arm_a_runner_only.phase_timings_ms.archive_write_ms.median",
                bmf,
            )


if __name__ == "__main__":
    unittest.main()
