#!/usr/bin/env python3
"""
Unit tests for compare.py against synthetic fixtures.

Run from the experiment package root:

    python3 -m unittest docs.experiments.runner-vs-otel-2026-05.compare.tests.test_compare

Or directly:

    python3 docs/experiments/runner-vs-otel-2026-05/compare/tests/test_compare.py
"""

from __future__ import annotations

import contextlib
import io
import json
import sys
import unittest
from pathlib import Path

# Allow importing compare.py directly when run as a script.
HERE = Path(__file__).resolve().parent
PKG = HERE.parent
sys.path.insert(0, str(PKG))

import compare  # noqa: E402


FIXTURES = HERE / "fixtures"
ARCHIVE_DIR = FIXTURES / "archive"
TRACE_FILE = FIXTURES / "trace.json"
TRACE_MATCHING_FILE = FIXTURES / "trace-matching-digest.json"
# Slice 3 fixtures
TRACE_TAMPERING_HONEST = FIXTURES / "trace-tampering.json"
TRACE_TAMPERING_MISMATCH = FIXTURES / "trace-tampering-mismatch.json"


class CompareTests(unittest.TestCase):
    def setUp(self) -> None:
        self.runner = compare.parse_runner_archive(ARCHIVE_DIR)
        self.trace = compare.parse_otlp_trace(TRACE_FILE)
        self.rows = compare.build_field_matrix(self.runner, self.trace)
        self.matrix = compare.matrix_to_json(self.rows, self.runner, self.trace)

    def test_archive_parsing(self) -> None:
        self.assertEqual(self.runner.run_id, "run_fixture_001")
        self.assertEqual(self.runner.schema, "assay.runner.archive_manifest.v0")
        self.assertTrue(self.runner.manifest_digest.startswith("sha256:"))
        self.assertEqual(self.runner.sdk_tools, ["read_file"])
        self.assertEqual(self.runner.sdk_tool_call_ids, ["tc_runner_policy_001"])
        self.assertEqual(self.runner.correlation_status, "clean")
        self.assertEqual(self.runner.observation_health["ringbuf_drops"], 0)
        self.assertEqual(
            self.runner.capability_surface["mcp_tools"], ["read_file"]
        )

    def test_trace_parsing(self) -> None:
        self.assertEqual(self.trace.run_id, "run_fixture_001")
        self.assertEqual(self.trace.gen_ai_provider, "openai")
        self.assertEqual(self.trace.gen_ai_request_model, "gpt-4o-mini")
        self.assertEqual(self.trace.gen_ai_response_model, "gpt-4o-mini")
        self.assertEqual(self.trace.gen_ai_input_tokens, 42)
        self.assertEqual(self.trace.gen_ai_output_tokens, 17)
        self.assertEqual(self.trace.tool_names, ["read_file"])
        self.assertEqual(self.trace.tool_call_ids, ["tc_runner_policy_001"])
        self.assertEqual(self.trace.correlation_status, "clean")
        self.assertEqual(self.trace.ringbuf_drops, 0)
        self.assertEqual(self.trace.span_count, 2)

    def test_tool_call_id_join(self) -> None:
        self.assertEqual(
            self.matrix["summary"]["tool_call_id_join"],
            "joined:tc_runner_policy_001",
        )

    def test_manifest_digest_binding_mismatch_is_explicit(self) -> None:
        # The fixture trace carries a placeholder digest that does not match
        # the actual fixture archive bytes. The comparator must surface this
        # as a mismatch, not silently treat it as a join.
        binding = self.matrix["summary"]["manifest_digest_binding"]
        self.assertTrue(binding.startswith("mismatch:"), binding)

    def test_field_matrix_row_count(self) -> None:
        # Seventeen well-known rows in v0 of the matrix (the Slice 3
        # intent-vs-effect row was added on top of the v1 sixteen);
        # bumping this is an explicit decision.
        self.assertEqual(len(self.rows), 17)

    def test_markdown_renders(self) -> None:
        md = compare.matrix_to_markdown(self.rows, self.matrix["summary"])
        # Smoke: the table header and at least one well-known row are present.
        self.assertIn("| Field | L1 Trace | L2 Archive |", md)
        self.assertIn("tool_call_id joinability", md)
        self.assertIn("manifest digest binding", md)


class OtlpShapeVarianceTests(unittest.TestCase):
    """
    Defensive coverage for OTLP/JSON shape variants the comparator must accept:

    - `intValue` carried as a native JSON number (some SDK exporters) instead of
      the spec-canonical JSON string;
    - `kind` carried as a numeric enum (1) instead of the `SPAN_KIND_*` string;
    - root span's `assay.archive.manifest_digest` attached to an event rather
      than a span attribute;
    - mixed: digest at root span, tool call id deeper in a child span.

    The matching-digest fixture exercises all four of the above, plus the
    happy path of a manifest-digest that lines up with the fixture archive
    so the binding row reports `tamper-evident-match`.
    """

    def test_matching_digest_reports_tamper_evident_match(self) -> None:
        runner = compare.parse_runner_archive(ARCHIVE_DIR)
        trace = compare.parse_otlp_trace(TRACE_MATCHING_FILE)
        rows = compare.build_field_matrix(runner, trace)
        matrix = compare.matrix_to_json(rows, runner, trace)
        self.assertEqual(
            matrix["summary"]["manifest_digest_binding"], "tamper-evident-match"
        )

    def test_int_value_as_native_number_is_accepted(self) -> None:
        # ringbuf_drops in the matching fixture is intValue: 0 (a real int,
        # not the canonical string-encoded form). parse_otlp_trace must still
        # populate the integer field.
        trace = compare.parse_otlp_trace(TRACE_MATCHING_FILE)
        self.assertEqual(trace.ringbuf_drops, 0)

    def test_digest_on_event_is_picked_up(self) -> None:
        # The matching fixture carries assay.archive.manifest_digest on the
        # `assay.archive.created` event, not on the root span's attribute
        # list. The parser must look in both places.
        trace = compare.parse_otlp_trace(TRACE_MATCHING_FILE)
        self.assertTrue(trace.manifest_digest.startswith("sha256:"))


class ExitCodeTests(unittest.TestCase):
    """
    The exit-code contract is the only stable surface CI / Harness flows can
    rely on. Lock it down with unit tests.
    """

    @staticmethod
    def _silent_main(argv: list[str]) -> int:
        # Suppress stdout/stderr so test output stays readable; we only care
        # about exit code here.
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(
            io.StringIO()
        ):
            return compare.main(argv)

    def test_match_with_require_binding_match_returns_zero(self) -> None:
        rc = self._silent_main(
            [
                "--archive",
                str(ARCHIVE_DIR),
                "--trace",
                str(TRACE_MATCHING_FILE),
                "--require-binding-match",
            ]
        )
        self.assertEqual(rc, compare.EXIT_OK)

    def test_mismatch_with_require_binding_match_returns_bad_evidence(self) -> None:
        rc = self._silent_main(
            [
                "--archive",
                str(ARCHIVE_DIR),
                "--trace",
                str(TRACE_FILE),
                "--require-binding-match",
            ]
        )
        self.assertEqual(rc, compare.EXIT_BAD_EVIDENCE)

    def test_mismatch_without_strict_flag_still_succeeds(self) -> None:
        # Default behavior: report the mismatch in the summary but do not fail.
        # This keeps Arm A and Arm B (and the synthetic fixture pair used by
        # other tests) usable.
        rc = self._silent_main(
            ["--archive", str(ARCHIVE_DIR), "--trace", str(TRACE_FILE)]
        )
        self.assertEqual(rc, compare.EXIT_OK)

    def test_missing_input_returns_bad_input(self) -> None:
        rc = self._silent_main(
            [
                "--archive",
                str(ARCHIVE_DIR),
                "--trace",
                "/nonexistent/trace.json",
            ]
        )
        self.assertEqual(rc, compare.EXIT_BAD_INPUT)


class IntentEffectTests(unittest.TestCase):
    """
    Slice 3: cover the reported-tool-argument vs measured-kernel-path
    comparator path. Three states matter:

      not-applicable    no sensitive content captured in trace
      intent-effect-match   reported path appears in archive's
                            capability_surface.filesystem_paths
      intent-effect-mismatch:<path>   reported path NOT in archive's
                                      paths (tampering signal)
    """

    def test_no_sensitive_capture_reports_not_applicable(self) -> None:
        runner = compare.parse_runner_archive(ARCHIVE_DIR)
        trace = compare.parse_otlp_trace(TRACE_FILE)
        matrix = compare.matrix_to_json(
            compare.build_field_matrix(runner, trace), runner, trace
        )
        self.assertEqual(
            matrix["summary"]["intent_effect_status"], "not-applicable"
        )

    def test_reported_path_in_measured_reports_match(self) -> None:
        # trace-tampering.json reports
        # /tmp/fixture/openai-agents-input.txt, which is exactly what the
        # fixture archive's capability_surface.filesystem_paths contains.
        runner = compare.parse_runner_archive(ARCHIVE_DIR)
        trace = compare.parse_otlp_trace(TRACE_TAMPERING_HONEST)
        matrix = compare.matrix_to_json(
            compare.build_field_matrix(runner, trace), runner, trace
        )
        self.assertEqual(
            matrix["summary"]["intent_effect_status"], "intent-effect-match"
        )

    def test_reported_path_not_in_measured_reports_mismatch(self) -> None:
        # trace-tampering-mismatch.json reports /workdir/safe.txt, which
        # is NOT in the fixture archive's filesystem paths. That's the
        # tampering signal: reported intent (safe.txt) diverges from
        # measured effect (openai-agents-input.txt).
        runner = compare.parse_runner_archive(ARCHIVE_DIR)
        trace = compare.parse_otlp_trace(TRACE_TAMPERING_MISMATCH)
        matrix = compare.matrix_to_json(
            compare.build_field_matrix(runner, trace), runner, trace
        )
        status = matrix["summary"]["intent_effect_status"]
        self.assertTrue(status.startswith("intent-effect-mismatch:"), status)
        self.assertIn("/workdir/safe.txt", status)

    def test_reported_paths_extracted_into_trace_observation(self) -> None:
        trace = compare.parse_otlp_trace(TRACE_TAMPERING_MISMATCH)
        self.assertEqual(len(trace.reported_tool_calls), 1)
        call = trace.reported_tool_calls[0]
        self.assertEqual(call["tool_call_id"], "tc_runner_policy_001")
        self.assertEqual(call["tool_name"], "read_file")
        self.assertEqual(call["arguments_path"], "/workdir/safe.txt")


if __name__ == "__main__":
    unittest.main(verbosity=2)
