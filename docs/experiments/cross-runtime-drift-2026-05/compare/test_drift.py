"""Unit tests for the cross-runtime drift comparator.

Stdlib unittest only. Exercises:
  - parse_archive happy path (directory + .tar.gz)
  - parse_archive failure modes (missing manifest, corrupt JSON,
    broken tar)
  - build_drift_report against the synthetic fixtures in
    compare/fixtures/{arm-a-openai, arm-b-gemini}/
  - classification labels per dimension
  - main() exit codes and file output

Run from repo root:
  python3 -m unittest discover \
    -s docs/experiments/cross-runtime-drift-2026-05/compare \
    -p 'test_*.py'

(`python3 -m unittest <module>` cannot be used because the
directory name contains a hyphen, which Python's module importer
rejects. Use the discover form above instead.)
"""
from __future__ import annotations

import io
import json
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path

THIS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(THIS_DIR))

import drift  # noqa: E402  (after sys.path tweak)

FIXTURES = THIS_DIR / "fixtures"
ARM_A = FIXTURES / "arm-a-openai"
ARM_B = FIXTURES / "arm-b-gemini"


def _make_tarball(src_dir: Path, dest: Path) -> Path:
    """Build a .tar.gz of src_dir's contents (paths relative to src_dir)."""
    with tarfile.open(dest, "w:gz") as tf:
        for path in sorted(src_dir.rglob("*")):
            if path.is_file():
                tf.add(path, arcname=str(path.relative_to(src_dir)))
    return dest


# ---------------------------------------------------------------------------
# parse_archive
# ---------------------------------------------------------------------------


class ParseArchiveHappyPathTests(unittest.TestCase):
    def test_parse_arm_a_directory(self) -> None:
        obs = drift.parse_archive(ARM_A)
        self.assertEqual(obs.runtime_label, "openai-agents")
        self.assertEqual(obs.run_id, "drift_fixture_a_openai_001")
        self.assertTrue(obs.manifest_digest.startswith("sha256:"))
        self.assertIn(
            "/tmp/work/fixture-input.txt",
            obs.capability_surface["filesystem_paths"],
        )
        self.assertEqual(
            obs.capability_surface["network_endpoints"],
            ["api.openai.com:443"],
        )
        self.assertEqual(obs.sdk_tools, ["read_file", "write_file"])
        self.assertEqual(
            obs.kernel_file_operations,
            [
                "create:/tmp/work/fixture-output.txt",
                "read:/tmp/work/fixture-input.txt",
                "truncate:/tmp/work/fixture-output.txt",
                "write:/tmp/work/fixture-output.txt",
            ],
        )
        # Ordering: read_file first (seq=0), then write_file (seq=2).
        self.assertEqual(
            obs.sdk_tool_order,
            ["tc_openai_001:read_file", "tc_openai_002:write_file"],
        )

    def test_parse_arm_b_directory(self) -> None:
        obs = drift.parse_archive(ARM_B)
        self.assertEqual(obs.runtime_label, "gemini-genai")
        self.assertEqual(
            obs.capability_surface["network_endpoints"],
            [
                "generativelanguage.googleapis.com:443",
                "oauth2.googleapis.com:443",
            ],
        )
        self.assertEqual(obs.sdk_tools, ["read_file", "write_file"])

    def test_parse_arm_a_as_tarball(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tarpath = Path(tmp) / "arm-a.tar.gz"
            _make_tarball(ARM_A, tarpath)
            obs = drift.parse_archive(tarpath)
            self.assertEqual(obs.runtime_label, "openai-agents")
            self.assertEqual(obs.sdk_event_count, 5)


class ParseArchiveFailureTests(unittest.TestCase):
    def test_missing_manifest_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(drift.BadArchiveError) as ctx:
                drift.parse_archive(Path(tmp))
            self.assertIn("manifest.json not found", str(ctx.exception))

    def test_corrupt_manifest_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmpdir = Path(tmp)
            (tmpdir / "manifest.json").write_text("{not valid", encoding="utf-8")
            with self.assertRaises(drift.BadArchiveError) as ctx:
                drift.parse_archive(tmpdir)
            self.assertIn("invalid JSON", str(ctx.exception))

    def test_corrupt_sdk_ndjson_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmpdir = Path(tmp)
            (tmpdir / "manifest.json").write_text(
                json.dumps({"schema": "x", "run_id": "y"}), encoding="utf-8"
            )
            # capability-surface.json is required (P1 #2 review); provide
            # a minimal one so the test exercises the *SDK* parse failure
            # rather than the missing-capability-surface gate.
            (tmpdir / "capability-surface.json").write_text(
                json.dumps({"schema": "assay.runner.capability_surface.v0"}),
                encoding="utf-8",
            )
            (tmpdir / "layers").mkdir()
            (tmpdir / "layers" / "sdk.ndjson").write_text(
                "{bad json}\n", encoding="utf-8"
            )
            with self.assertRaises(drift.BadArchiveError) as ctx:
                drift.parse_archive(tmpdir)
            self.assertIn("invalid JSON", str(ctx.exception))

    def test_nonexistent_archive_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(drift.BadArchiveError) as ctx:
                drift.parse_archive(Path(tmp) / "does-not-exist.tar.gz")
            self.assertIn("does not exist", str(ctx.exception))

    def test_missing_capability_surface_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmpdir = Path(tmp)
            (tmpdir / "manifest.json").write_text(
                json.dumps({"schema": "x", "run_id": "y"}), encoding="utf-8"
            )
            # No capability-surface.json — must be a hard exit-3, not a
            # silent "everything inconclusive" report.
            with self.assertRaises(drift.BadArchiveError) as ctx:
                drift.parse_archive(tmpdir)
            self.assertIn("capability-surface.json", str(ctx.exception))


# ---------------------------------------------------------------------------
# build_drift_report
# ---------------------------------------------------------------------------


class DriftReportClassificationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.a = drift.parse_archive(ARM_A)
        self.b = drift.parse_archive(ARM_B)
        self.fixture_paths = frozenset(
            [
                "/tmp/work/fixture-input.txt",
                "/tmp/work/fixture-output.txt",
            ]
        )

    def _by_dim(
        self, rows: list[drift.DriftRow]
    ) -> dict[str, drift.DriftRow]:
        return {r.dimension: r for r in rows}

    def test_filesystem_paths_runtime_induced(self) -> None:
        rows = drift.build_drift_report(
            self.a, self.b, fixture_paths=self.fixture_paths
        )
        row = self._by_dim(rows)["filesystem_paths_touched"]
        # Both arms touched the two fixture paths.
        self.assertIn("/tmp/work/fixture-input.txt", row.in_both)
        self.assertIn("/tmp/work/fixture-output.txt", row.in_both)
        # Non-shared paths exist on both sides (different node_modules).
        self.assertTrue(row.only_in_a)
        self.assertTrue(row.only_in_b)
        self.assertEqual(row.classification, drift.CLASSIFICATION_RUNTIME)

    def test_network_endpoints_provider_induced(self) -> None:
        rows = drift.build_drift_report(self.a, self.b)
        row = self._by_dim(rows)["network_endpoints"]
        self.assertEqual(row.in_both, [])
        self.assertIn("api.openai.com:443", row.only_in_a)
        self.assertIn(
            "generativelanguage.googleapis.com:443", row.only_in_b
        )
        self.assertEqual(row.classification, drift.CLASSIFICATION_PROVIDER)

    def test_kernel_file_operations_task_induced(self) -> None:
        rows = drift.build_drift_report(
            self.a, self.b, fixture_paths=self.fixture_paths
        )
        row = self._by_dim(rows)["kernel_file_operations"]
        self.assertEqual(
            row.in_both,
            [
                "create:/tmp/work/fixture-output.txt",
                "read:/tmp/work/fixture-input.txt",
                "truncate:/tmp/work/fixture-output.txt",
                "write:/tmp/work/fixture-output.txt",
            ],
        )
        self.assertEqual(row.classification, drift.CLASSIFICATION_TASK)

    def test_process_execs_task_induced(self) -> None:
        rows = drift.build_drift_report(self.a, self.b)
        row = self._by_dim(rows)["process_execs"]
        self.assertEqual(row.in_both, ["/usr/bin/node"])
        self.assertEqual(row.classification, drift.CLASSIFICATION_TASK)

    def test_sdk_tools_task_induced(self) -> None:
        rows = drift.build_drift_report(self.a, self.b)
        row = self._by_dim(rows)["sdk_tool_events"]
        self.assertEqual(row.in_both, ["read_file", "write_file"])
        self.assertEqual(row.classification, drift.CLASSIFICATION_TASK)

    def test_mcp_empty_both_inconclusive(self) -> None:
        rows = drift.build_drift_report(self.a, self.b)
        row = self._by_dim(rows)["mcp_tool_surface"]
        self.assertEqual(row.in_both, [])
        self.assertEqual(
            row.classification, drift.CLASSIFICATION_INCONCLUSIVE
        )

    def test_tool_invocation_order_task_induced(self) -> None:
        rows = drift.build_drift_report(self.a, self.b)
        row = self._by_dim(rows)["tool_invocation_order"]
        # Both arms invoked read_file then write_file.
        self.assertEqual(row.in_both, ["read_file", "write_file"])
        self.assertEqual(row.classification, drift.CLASSIFICATION_TASK)


class DriftReportInconclusiveTests(unittest.TestCase):
    """One arm has data for a dimension, the other does not → inconclusive."""

    def test_one_sided_network_endpoints(self) -> None:
        a = drift.ArchiveObservation(
            path="a",
            run_id="a",
            runtime_label="openai-agents",
            manifest_digest="sha256:aa",
            capability_surface={
                "filesystem_paths": [],
                "network_endpoints": ["api.openai.com:443"],
                "process_execs": [],
                "mcp_tools": [],
                "policy_decisions": [],
            },
            sdk_events=[],
            sdk_event_count=0,
            sdk_tools=[],
            sdk_tool_call_ids=[],
            sdk_tool_order=[],
        )
        b = drift.ArchiveObservation(
            path="b",
            run_id="b",
            runtime_label="gemini-genai",
            manifest_digest="sha256:bb",
            capability_surface={
                "filesystem_paths": [],
                "network_endpoints": [],
                "process_execs": [],
                "mcp_tools": [],
                "policy_decisions": [],
            },
            sdk_events=[],
            sdk_event_count=0,
            sdk_tools=[],
            sdk_tool_call_ids=[],
            sdk_tool_order=[],
        )
        rows = drift.build_drift_report(a, b)
        row = next(r for r in rows if r.dimension == "network_endpoints")
        self.assertEqual(
            row.classification, drift.CLASSIFICATION_INCONCLUSIVE
        )
        self.assertIn("arm-b", row.detail)


class DriftReportFixturePathOverrideTests(unittest.TestCase):
    """An extra fixture path that only one arm touched (e.g. cache file)
    should be classified task-induced when whitelisted, runtime-induced
    otherwise."""

    def _make_obs(self, label: str, extra_path: str) -> drift.ArchiveObservation:
        return drift.ArchiveObservation(
            path=label,
            run_id=label,
            runtime_label=label,
            manifest_digest="sha256:" + "0" * 64,
            capability_surface={
                "filesystem_paths": sorted(
                    [
                        "/tmp/work/fixture-input.txt",
                        "/tmp/work/fixture-output.txt",
                        extra_path,
                    ]
                ),
                "network_endpoints": [],
                "process_execs": [],
                "mcp_tools": [],
                "policy_decisions": [],
            },
            sdk_events=[],
            sdk_event_count=0,
            sdk_tools=[],
            sdk_tool_call_ids=[],
            sdk_tool_order=[],
        )

    def test_fixture_path_override_classifies_as_task(self) -> None:
        a = self._make_obs(
            "openai-agents", "/tmp/work/extra-fixture.txt"
        )
        b = self._make_obs(
            "gemini-genai", "/tmp/work/fixture-input.txt"
        )  # b has no extra; a does
        rows = drift.build_drift_report(
            a,
            b,
            fixture_paths=frozenset(
                [
                    "/tmp/work/fixture-input.txt",
                    "/tmp/work/fixture-output.txt",
                    "/tmp/work/extra-fixture.txt",
                ]
            ),
        )
        row = next(
            r for r in rows if r.dimension == "filesystem_paths_touched"
        )
        # Only difference is the extra-fixture path which is whitelisted.
        self.assertIn("/tmp/work/extra-fixture.txt", row.only_in_a)
        self.assertEqual(row.classification, drift.CLASSIFICATION_TASK)


# ---------------------------------------------------------------------------
# main()
# ---------------------------------------------------------------------------


class MainCliTests(unittest.TestCase):
    def test_main_writes_json_and_md(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmpdir = Path(tmp)
            out_json = tmpdir / "drift.json"
            out_md = tmpdir / "drift.md"
            rc = drift.main(
                [
                    "--archive-a",
                    str(ARM_A),
                    "--archive-b",
                    str(ARM_B),
                    "--out-json",
                    str(out_json),
                    "--out-md",
                    str(out_md),
                    "--fixture-path",
                    "/tmp/work/fixture-input.txt",
                    "--fixture-path",
                    "/tmp/work/fixture-output.txt",
                ]
            )
            self.assertEqual(rc, 0)
            self.assertTrue(out_json.is_file())
            self.assertTrue(out_md.is_file())
            payload = json.loads(out_json.read_text(encoding="utf-8"))
            self.assertEqual(payload["schema"], drift.DRIFT_REPORT_SCHEMA)
            self.assertEqual(
                payload["archive_a"]["runtime_label"], "openai-agents"
            )
            self.assertEqual(
                payload["archive_b"]["runtime_label"], "gemini-genai"
            )
            dims = [r["dimension"] for r in payload["rows"]]
            self.assertIn("filesystem_paths_touched", dims)
            self.assertIn("kernel_file_operations", dims)
            self.assertIn("network_endpoints", dims)
            self.assertIn("tool_invocation_order", dims)
            # Markdown carries a header + a row per dimension.
            md = out_md.read_text(encoding="utf-8")
            self.assertIn("# Cross-Runtime Drift Report", md)
            self.assertIn("filesystem_paths_touched", md)

    def test_main_returns_3_on_bad_archive(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            rc = drift.main(
                [
                    "--archive-a",
                    tmp,  # empty dir, no manifest
                    "--archive-b",
                    str(ARM_B),
                ]
            )
            self.assertEqual(rc, 3)

    def test_main_returns_3_on_nonexistent_archive(self) -> None:
        """The CLI must not crash with a traceback when a path is wrong
        — that was P1 #1 in the Slice 2 review."""
        with tempfile.TemporaryDirectory() as tmp:
            rc = drift.main(
                [
                    "--archive-a",
                    str(Path(tmp) / "does-not-exist.tar.gz"),
                    "--archive-b",
                    str(ARM_B),
                ]
            )
            self.assertEqual(rc, 3)


class RuntimeLabelDerivationTests(unittest.TestCase):
    """runtime_label must come from SDK events' `source` field, not from
    a made-up manifest field (P2 #3 in the Slice 2 review)."""

    def test_label_derived_from_sdk_event_source(self) -> None:
        a = drift.parse_archive(ARM_A)
        b = drift.parse_archive(ARM_B)
        self.assertEqual(a.runtime_label, "openai-agents")
        self.assertEqual(b.runtime_label, "gemini-genai")

    def test_label_is_none_when_no_sdk_events(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmpdir = Path(tmp)
            (tmpdir / "manifest.json").write_text(
                json.dumps({"schema": "x", "run_id": "y"}), encoding="utf-8"
            )
            (tmpdir / "capability-surface.json").write_text(
                json.dumps({"schema": "assay.runner.capability_surface.v0"}),
                encoding="utf-8",
            )
            obs = drift.parse_archive(tmpdir)
            self.assertIsNone(obs.runtime_label)


class ProviderClassificationScopeTests(unittest.TestCase):
    """Provider-host classification must only apply to the
    network_endpoints dimension. A filesystem path that happens to
    contain a provider hostname must NOT be labelled provider-induced
    (P2 #1 in the Slice 2 review)."""

    def _make_obs(
        self, label: str, fs_extra: str
    ) -> drift.ArchiveObservation:
        return drift.ArchiveObservation(
            path=label,
            run_id=label,
            runtime_label=label,
            manifest_digest="sha256:" + "0" * 64,
            capability_surface={
                "filesystem_paths": sorted(
                    [
                        "/tmp/work/fixture-input.txt",
                        "/tmp/work/fixture-output.txt",
                        fs_extra,
                    ]
                ),
                "network_endpoints": [],
                "process_execs": [],
                "mcp_tools": [],
                "policy_decisions": [],
            },
            sdk_events=[],
            sdk_event_count=0,
            sdk_tools=[],
            sdk_tool_call_ids=[],
            sdk_tool_order=[],
        )

    def test_provider_hostname_in_fs_path_is_runtime_not_provider(
        self,
    ) -> None:
        # A filesystem path that contains 'api.openai.com' must not
        # bleed into the provider-host classification.
        a = self._make_obs(
            "openai-agents",
            "/tmp/cache/api.openai.com.json",
        )
        b = self._make_obs(
            "gemini-genai",
            "/tmp/cache/generativelanguage.googleapis.com.json",
        )
        rows = drift.build_drift_report(a, b)
        row = next(
            r for r in rows if r.dimension == "filesystem_paths_touched"
        )
        self.assertEqual(row.classification, drift.CLASSIFICATION_RUNTIME)
        # And the detail must NOT claim "all match provider whitelist".
        self.assertNotIn("provider", row.detail)


class NetworkEndpointParsingTests(unittest.TestCase):
    """Provider-host check must parse `host:port` properly and reject
    non-matching substrings."""

    def test_host_port_split(self) -> None:
        self.assertTrue(
            drift._network_endpoint_matches_provider(
                "api.openai.com:443", drift.DEFAULT_PROVIDER_HOSTS
            )
        )

    def test_subdomain_match(self) -> None:
        self.assertTrue(
            drift._network_endpoint_matches_provider(
                "auth.openai.com:443", drift.DEFAULT_PROVIDER_HOSTS
            )
        )

    def test_substring_does_not_match(self) -> None:
        # A path-shaped string containing the host should not match.
        self.assertFalse(
            drift._network_endpoint_matches_provider(
                "/tmp/api.openai.com.json", drift.DEFAULT_PROVIDER_HOSTS
            )
        )

    def test_lookalike_host_does_not_match(self) -> None:
        # `evil-api.openai.com.attacker.example` must NOT be accepted.
        self.assertFalse(
            drift._network_endpoint_matches_provider(
                "evil-api.openai.com.attacker.example:443",
                drift.DEFAULT_PROVIDER_HOSTS,
            )
        )


class MarkdownEscapeTests(unittest.TestCase):
    """Markdown table cells must escape `|` so that a runtime-induced
    invocation-order row with `a: ... | b: ...` in the detail does not
    break the table (P2 #2 in the Slice 2 review)."""

    def test_pipe_in_detail_is_escaped(self) -> None:
        a = drift.ArchiveObservation(
            path="a",
            run_id="a",
            runtime_label="openai-agents",
            manifest_digest="sha256:" + "0" * 64,
            capability_surface={
                "filesystem_paths": [],
                "network_endpoints": [],
                "process_execs": [],
                "mcp_tools": [],
                "policy_decisions": [],
            },
            sdk_events=[],
            sdk_event_count=2,
            sdk_tools=["read_file", "write_file"],
            sdk_tool_call_ids=["tc_1", "tc_2"],
            sdk_tool_order=["tc_1:read_file", "tc_2:write_file"],
        )
        b = drift.ArchiveObservation(
            path="b",
            run_id="b",
            runtime_label="gemini-genai",
            manifest_digest="sha256:" + "0" * 64,
            capability_surface={
                "filesystem_paths": [],
                "network_endpoints": [],
                "process_execs": [],
                "mcp_tools": [],
                "policy_decisions": [],
            },
            sdk_events=[],
            sdk_event_count=2,
            sdk_tools=["read_file", "write_file"],
            sdk_tool_call_ids=["tc_1", "tc_2"],
            # Reversed order to trigger the runtime-induced detail
            # that contains "a: ... | b: ...".
            sdk_tool_order=["tc_1:write_file", "tc_2:read_file"],
        )
        rows = drift.build_drift_report(a, b)
        md = drift.report_to_md(a, b, rows)
        # Every line that starts with `|` must have the same number of
        # unescaped `|` separators — otherwise the table breaks. We
        # count by stripping escaped pipes first.
        for line in md.splitlines():
            if not line.startswith("|"):
                continue
            unescaped = line.replace("\\|", "")
            # Each table row in our schema has 8 unescaped `|` chars
            # (7 cell separators + leading and trailing pipe = 8).
            count = unescaped.count("|")
            self.assertEqual(
                count,
                8,
                f"row has {count} unescaped pipes, expected 8: {line!r}",
            )


if __name__ == "__main__":
    unittest.main()
