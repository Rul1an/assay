#!/usr/bin/env python3
"""
Runner-vs-OTel field-matrix comparator.

Inputs:
  - A Runner archive (.tar.gz) OR an already-extracted archive directory.
    The archive must follow the assay.runner.archive_manifest.v0 layout
    documented in crates/assay-runner-schema/src/archive_manifest.rs.
  - An OpenTelemetry trace export as JSON (OTLP/JSON wire format).
    The standard `resourceSpans -> scopeSpans -> spans` nesting is supported.

Outputs:
  - A normalized JSON document with the per-field observation matrix.
  - A Markdown table summarizing the same matrix for embedding in the
    experiment write-up.

This script is deliberately dependency-free (Python stdlib only) so it runs
in CI and on the delegated host without an extra setup step. Schema knowledge
is encoded inline against the v0 contracts.

Status: v1 of the runner-vs-otel-2026-05 experiment package.
Non-claims: this script does not interpret policy, evaluate acceptability,
or rank tools. It compares evidence shape and joinability.
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import sys
import tarfile
from pathlib import Path
from typing import Any, Iterator


ARCHIVE_MANIFEST_SCHEMA = "assay.runner.archive_manifest.v0"
CAPABILITY_SURFACE_SCHEMA = "assay.runner.capability_surface.v0"
OBSERVATION_HEALTH_SCHEMA = "assay.runner.observation_health.v0"
CORRELATION_REPORT_SCHEMA = "assay.runner.correlation_report.v0"

MANIFEST_PATH = "manifest.json"
CAPABILITY_SURFACE_PATH = "capability-surface.json"
OBSERVATION_HEALTH_PATH = "observation-health.json"
CORRELATION_REPORT_PATH = "correlation-report.json"
SDK_LAYER_PATH = "layers/sdk.ndjson"


# ---------------------------------------------------------------------------
# Normalized observation model
# ---------------------------------------------------------------------------


@dataclasses.dataclass
class RunnerObservation:
    """What we extract from a Runner archive."""

    run_id: str | None
    schema: str | None
    manifest_digest: str | None
    capability_surface: dict[str, list[str]]
    observation_health: dict[str, Any]
    correlation_status: str | None
    sdk_tool_call_ids: list[str]
    sdk_tools: list[str]
    sdk_event_count: int


@dataclasses.dataclass
class TraceObservation:
    """What we extract from an OTLP JSON trace."""

    run_id: str | None
    manifest_digest: str | None
    archive_schema: str | None
    correlation_status: str | None
    ringbuf_drops: int | None
    gen_ai_provider: str | None
    gen_ai_request_model: str | None
    gen_ai_response_model: str | None
    gen_ai_input_tokens: int | None
    gen_ai_output_tokens: int | None
    tool_call_ids: list[str]
    tool_names: list[str]
    span_count: int


# ---------------------------------------------------------------------------
# Runner archive parsing
# ---------------------------------------------------------------------------


def _open_archive_member(source: Path, member: str) -> bytes | None:
    """
    Return the bytes of a member file from either an extracted directory
    or a tarball, or None if the member is missing.
    """
    if source.is_dir():
        path = source / member
        if not path.exists():
            return None
        return path.read_bytes()

    with tarfile.open(source, "r:*") as tf:
        try:
            extracted = tf.extractfile(member)
        except KeyError:
            return None
        if extracted is None:
            return None
        return extracted.read()


def parse_runner_archive(source: Path) -> RunnerObservation:
    """
    Parse a Runner measured-run archive.

    Accepts either a path to the .tar.gz produced by `assay runner-spike run`
    or a path to an already-extracted directory tree (handy for unit tests
    and for inspecting an archive without re-tarring it).

    The manifest digest is computed over the **exact** `manifest.json` bytes
    pulled out of the archive (tar or directory), never over re-serialized
    JSON. Re-serializing would silently rewrite key order, whitespace, and
    Unicode escapes; the digest would still be deterministic but it would
    no longer match the digest computed by `manifest-binding.ts` on the
    trace side, breaking the tamper-evident binding.
    """
    manifest_bytes = _open_archive_member(source, MANIFEST_PATH)
    if manifest_bytes is None:
        raise FileNotFoundError(f"manifest.json not found in {source}")

    manifest_digest = _sha256_hex(manifest_bytes)
    manifest = json.loads(manifest_bytes.decode("utf-8"))

    schema = manifest.get("schema")
    run_id = manifest.get("run_id")

    capability_surface: dict[str, list[str]] = {}
    capability_bytes = _open_archive_member(source, CAPABILITY_SURFACE_PATH)
    if capability_bytes is not None:
        capability = json.loads(capability_bytes.decode("utf-8"))
        for key in (
            "filesystem_paths",
            "network_endpoints",
            "process_execs",
            "mcp_tools",
            "policy_decisions",
        ):
            value = capability.get(key, [])
            capability_surface[key] = sorted(list(value))

    observation_health: dict[str, Any] = {}
    health_bytes = _open_archive_member(source, OBSERVATION_HEALTH_PATH)
    if health_bytes is not None:
        observation_health = json.loads(health_bytes.decode("utf-8"))

    correlation_status: str | None = None
    correlation_bytes = _open_archive_member(source, CORRELATION_REPORT_PATH)
    if correlation_bytes is not None:
        correlation_report = json.loads(correlation_bytes.decode("utf-8"))
        correlation_status = correlation_report.get("status")

    sdk_tool_call_ids: list[str] = []
    sdk_tools: list[str] = []
    sdk_event_count = 0
    sdk_bytes = _open_archive_member(source, SDK_LAYER_PATH)
    if sdk_bytes is not None:
        for line in sdk_bytes.decode("utf-8").splitlines():
            if not line.strip():
                continue
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue
            sdk_event_count += 1
            call_id = event.get("tool_call_id")
            if isinstance(call_id, str) and call_id and call_id not in sdk_tool_call_ids:
                sdk_tool_call_ids.append(call_id)
            tool = event.get("tool")
            if isinstance(tool, str) and tool and tool not in sdk_tools:
                sdk_tools.append(tool)

    return RunnerObservation(
        run_id=run_id,
        schema=schema,
        manifest_digest=f"sha256:{manifest_digest}",
        capability_surface=capability_surface,
        observation_health=observation_health,
        correlation_status=correlation_status,
        sdk_tool_call_ids=sorted(sdk_tool_call_ids),
        sdk_tools=sorted(sdk_tools),
        sdk_event_count=sdk_event_count,
    )


def _sha256_hex(data: bytes) -> str:
    import hashlib

    return hashlib.sha256(data).hexdigest()


# ---------------------------------------------------------------------------
# OTLP trace parsing
# ---------------------------------------------------------------------------


def _iter_spans(trace_doc: dict[str, Any]) -> Iterator[dict[str, Any]]:
    """
    Yield every span across resourceSpans/scopeSpans nesting.
    Supports the standard OTLP/JSON wire shape.
    """
    for resource in trace_doc.get("resourceSpans", []):
        for scope in resource.get("scopeSpans", []):
            for span in scope.get("spans", []):
                yield span


def _attr_value(attribute: dict[str, Any]) -> Any:
    """Decode an OTLP attribute value to a plain Python type."""
    value = attribute.get("value", {})
    if "stringValue" in value:
        return value["stringValue"]
    if "intValue" in value:
        return int(value["intValue"])
    if "doubleValue" in value:
        return float(value["doubleValue"])
    if "boolValue" in value:
        return bool(value["boolValue"])
    if "arrayValue" in value:
        return [_attr_value({"value": v}) for v in value["arrayValue"].get("values", [])]
    return None


def _attrs_dict(span: dict[str, Any]) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for attr in span.get("attributes", []):
        key = attr.get("key")
        if key:
            out[key] = _attr_value(attr)
    return out


def _event_attrs(event: dict[str, Any]) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for attr in event.get("attributes", []):
        key = attr.get("key")
        if key:
            out[key] = _attr_value(attr)
    return out


def parse_otlp_trace(path: Path) -> TraceObservation:
    """
    Parse an OTLP/JSON trace export.

    Looks for:
      - `assay.run.id` attribute on any span or event.
      - `assay.archive.manifest_digest` and friends on the manifest-binding event
        or on the root span.
      - OTel GenAI semantic attributes: `gen_ai.provider.name`,
        `gen_ai.request.model`, `gen_ai.response.model`,
        `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens`,
        `gen_ai.tool.name`, `gen_ai.tool.call.id`.
    """
    trace_doc = json.loads(path.read_text(encoding="utf-8"))

    run_id: str | None = None
    manifest_digest: str | None = None
    archive_schema: str | None = None
    correlation_status: str | None = None
    ringbuf_drops: int | None = None

    gen_ai_provider: str | None = None
    gen_ai_request_model: str | None = None
    gen_ai_response_model: str | None = None
    gen_ai_input_tokens: int | None = None
    gen_ai_output_tokens: int | None = None
    tool_call_ids: list[str] = []
    tool_names: list[str] = []

    span_count = 0

    for span in _iter_spans(trace_doc):
        span_count += 1
        attrs = _attrs_dict(span)

        run_id = run_id or attrs.get("assay.run.id")
        manifest_digest = manifest_digest or attrs.get("assay.archive.manifest_digest")
        archive_schema = archive_schema or attrs.get("assay.archive.schema")
        correlation_status = correlation_status or attrs.get(
            "assay.runner.correlation_status"
        )
        if "assay.runner.ringbuf_drops" in attrs and ringbuf_drops is None:
            try:
                ringbuf_drops = int(attrs["assay.runner.ringbuf_drops"])
            except (TypeError, ValueError):
                pass

        gen_ai_provider = gen_ai_provider or attrs.get("gen_ai.provider.name")
        gen_ai_request_model = gen_ai_request_model or attrs.get("gen_ai.request.model")
        gen_ai_response_model = gen_ai_response_model or attrs.get(
            "gen_ai.response.model"
        )
        if "gen_ai.usage.input_tokens" in attrs and gen_ai_input_tokens is None:
            try:
                gen_ai_input_tokens = int(attrs["gen_ai.usage.input_tokens"])
            except (TypeError, ValueError):
                pass
        if "gen_ai.usage.output_tokens" in attrs and gen_ai_output_tokens is None:
            try:
                gen_ai_output_tokens = int(attrs["gen_ai.usage.output_tokens"])
            except (TypeError, ValueError):
                pass

        call_id = attrs.get("gen_ai.tool.call.id")
        if isinstance(call_id, str) and call_id and call_id not in tool_call_ids:
            tool_call_ids.append(call_id)
        tool_name = attrs.get("gen_ai.tool.name")
        if isinstance(tool_name, str) and tool_name and tool_name not in tool_names:
            tool_names.append(tool_name)

        for event in span.get("events", []):
            event_attrs = _event_attrs(event)
            run_id = run_id or event_attrs.get("assay.run.id")
            manifest_digest = manifest_digest or event_attrs.get(
                "assay.archive.manifest_digest"
            )
            archive_schema = archive_schema or event_attrs.get("assay.archive.schema")
            correlation_status = correlation_status or event_attrs.get(
                "assay.runner.correlation_status"
            )
            if (
                "assay.runner.ringbuf_drops" in event_attrs
                and ringbuf_drops is None
            ):
                try:
                    ringbuf_drops = int(event_attrs["assay.runner.ringbuf_drops"])
                except (TypeError, ValueError):
                    pass

    return TraceObservation(
        run_id=run_id,
        manifest_digest=manifest_digest,
        archive_schema=archive_schema,
        correlation_status=correlation_status,
        ringbuf_drops=ringbuf_drops,
        gen_ai_provider=gen_ai_provider,
        gen_ai_request_model=gen_ai_request_model,
        gen_ai_response_model=gen_ai_response_model,
        gen_ai_input_tokens=gen_ai_input_tokens,
        gen_ai_output_tokens=gen_ai_output_tokens,
        tool_call_ids=sorted(tool_call_ids),
        tool_names=sorted(tool_names),
        span_count=span_count,
    )


# ---------------------------------------------------------------------------
# Field matrix
# ---------------------------------------------------------------------------


@dataclasses.dataclass
class FieldRow:
    field: str
    l1_trace: str  # "present" | "absent" | "n/a" | value
    l2_archive: str
    join_key: str
    claim_class: str
    notes: str = ""


def _bool_to_present(v: Any) -> str:
    if v is None:
        return "absent"
    if isinstance(v, (list, dict)) and len(v) == 0:
        return "empty"
    return "present"


def _join_status(runner_ids: list[str], trace_ids: list[str]) -> str:
    if not runner_ids and not trace_ids:
        return "no-tool-calls-observed"
    if not trace_ids:
        return "trace-side-absent"
    if not runner_ids:
        return "archive-side-absent"
    intersection = sorted(set(runner_ids) & set(trace_ids))
    if intersection:
        return f"joined:{','.join(intersection)}"
    return "ids-divergent"


def _digest_binding_status(
    archive_digest: str | None, trace_digest: str | None
) -> str:
    if archive_digest is None:
        return "archive-digest-missing"
    if trace_digest is None:
        return "trace-attribute-absent"
    if archive_digest.lower() == trace_digest.lower():
        return "tamper-evident-match"
    return f"mismatch:archive={archive_digest},trace={trace_digest}"


def build_field_matrix(
    runner: RunnerObservation, trace: TraceObservation
) -> list[FieldRow]:
    rows: list[FieldRow] = []

    rows.append(
        FieldRow(
            field="run identity (run_id)",
            l1_trace=trace.run_id or "absent",
            l2_archive=runner.run_id or "absent",
            join_key="assay.run.id",
            claim_class="correlation",
        )
    )
    rows.append(
        FieldRow(
            field="archive schema",
            l1_trace=trace.archive_schema or "absent",
            l2_archive=runner.schema or "absent",
            join_key="assay.archive.schema",
            claim_class="provenance",
        )
    )
    rows.append(
        FieldRow(
            field="manifest digest binding",
            l1_trace=trace.manifest_digest or "absent",
            l2_archive=runner.manifest_digest or "absent",
            join_key=_digest_binding_status(
                runner.manifest_digest, trace.manifest_digest
            ),
            claim_class="tamper-evident binding",
        )
    )
    rows.append(
        FieldRow(
            field="GenAI provider",
            l1_trace=trace.gen_ai_provider or "absent",
            l2_archive="n/a (trace-side concept)",
            join_key="span",
            claim_class="provenance",
        )
    )
    rows.append(
        FieldRow(
            field="GenAI request model",
            l1_trace=trace.gen_ai_request_model or "absent",
            l2_archive="n/a",
            join_key="span",
            claim_class="provenance",
        )
    )
    rows.append(
        FieldRow(
            field="GenAI response model",
            l1_trace=trace.gen_ai_response_model or "absent",
            l2_archive="n/a",
            join_key="span",
            claim_class="provenance",
        )
    )
    rows.append(
        FieldRow(
            field="GenAI input tokens",
            l1_trace=(
                str(trace.gen_ai_input_tokens)
                if trace.gen_ai_input_tokens is not None
                else "absent"
            ),
            l2_archive="n/a (archive does not measure tokens)",
            join_key="span",
            claim_class="cost/context",
        )
    )
    rows.append(
        FieldRow(
            field="GenAI output tokens",
            l1_trace=(
                str(trace.gen_ai_output_tokens)
                if trace.gen_ai_output_tokens is not None
                else "absent"
            ),
            l2_archive="n/a",
            join_key="span",
            claim_class="cost/context",
        )
    )
    rows.append(
        FieldRow(
            field="tool names",
            l1_trace=(",".join(trace.tool_names) if trace.tool_names else "absent"),
            l2_archive=(
                ",".join(runner.sdk_tools) if runner.sdk_tools else "absent"
            ),
            join_key="tool name",
            claim_class="joinable behavior",
        )
    )
    rows.append(
        FieldRow(
            field="tool_call_id joinability",
            l1_trace=(
                ",".join(trace.tool_call_ids) if trace.tool_call_ids else "absent"
            ),
            l2_archive=(
                ",".join(runner.sdk_tool_call_ids)
                if runner.sdk_tool_call_ids
                else "absent"
            ),
            join_key=_join_status(runner.sdk_tool_call_ids, trace.tool_call_ids),
            claim_class="primary join key",
        )
    )
    rows.append(
        FieldRow(
            field="filesystem paths",
            l1_trace="n/a (not in trace contract)",
            l2_archive=_bool_to_present(
                runner.capability_surface.get("filesystem_paths")
            ),
            join_key="none",
            claim_class="measured effect; bounded negative if health is clean",
            notes=", ".join(
                runner.capability_surface.get("filesystem_paths", [])[:3]
            ),
        )
    )
    rows.append(
        FieldRow(
            field="network endpoints",
            l1_trace="n/a (not in trace contract)",
            l2_archive=_bool_to_present(
                runner.capability_surface.get("network_endpoints")
            ),
            join_key="none",
            claim_class="measured effect",
        )
    )
    rows.append(
        FieldRow(
            field="process execs",
            l1_trace="n/a",
            l2_archive=_bool_to_present(
                runner.capability_surface.get("process_execs")
            ),
            join_key="none",
            claim_class="measured effect",
        )
    )
    rows.append(
        FieldRow(
            field="policy decisions",
            l1_trace="n/a (unless custom)",
            l2_archive=_bool_to_present(
                runner.capability_surface.get("policy_decisions")
            ),
            join_key="tool_call_id when present",
            claim_class="enforcement",
        )
    )
    rows.append(
        FieldRow(
            field="ringbuf_drops",
            l1_trace=(
                str(trace.ringbuf_drops) if trace.ringbuf_drops is not None else "absent"
            ),
            l2_archive=str(runner.observation_health.get("ringbuf_drops", "absent")),
            join_key="archive health",
            claim_class="measurement integrity",
        )
    )
    rows.append(
        FieldRow(
            field="cgroup correlation status",
            l1_trace=trace.correlation_status or "absent",
            l2_archive=runner.correlation_status or "absent",
            join_key="assay.runner.correlation_status",
            claim_class="measurement integrity",
        )
    )

    return rows


# ---------------------------------------------------------------------------
# Renderers
# ---------------------------------------------------------------------------


def matrix_to_json(
    rows: list[FieldRow], runner: RunnerObservation, trace: TraceObservation
) -> dict[str, Any]:
    return {
        "schema": "assay.experiment.runner_vs_otel.field_matrix.v0",
        "summary": {
            "trace_spans": trace.span_count,
            "archive_sdk_events": runner.sdk_event_count,
            "manifest_digest_binding": _digest_binding_status(
                runner.manifest_digest, trace.manifest_digest
            ),
            "tool_call_id_join": _join_status(
                runner.sdk_tool_call_ids, trace.tool_call_ids
            ),
        },
        "runner_observation": dataclasses.asdict(runner),
        "trace_observation": dataclasses.asdict(trace),
        "rows": [dataclasses.asdict(row) for row in rows],
    }


def matrix_to_markdown(rows: list[FieldRow], summary: dict[str, Any]) -> str:
    out: list[str] = []
    out.append("## Field Matrix")
    out.append("")
    out.append(f"- trace spans: {summary['trace_spans']}")
    out.append(f"- archive SDK events: {summary['archive_sdk_events']}")
    out.append(
        f"- manifest-digest binding: {summary['manifest_digest_binding']}"
    )
    out.append(f"- tool_call_id join: {summary['tool_call_id_join']}")
    out.append("")
    out.append("| Field | L1 Trace | L2 Archive | Join | Claim class | Notes |")
    out.append("|---|---|---|---|---|---|")
    for row in rows:
        out.append(
            "| {field} | {l1} | {l2} | {join} | {claim} | {notes} |".format(
                field=row.field,
                l1=_escape_md(row.l1_trace),
                l2=_escape_md(row.l2_archive),
                join=_escape_md(row.join_key),
                claim=_escape_md(row.claim_class),
                notes=_escape_md(row.notes),
            )
        )
    out.append("")
    return "\n".join(out)


def _escape_md(value: str) -> str:
    return value.replace("|", "\\|").replace("\n", " ")


# ---------------------------------------------------------------------------
# Exit codes
# ---------------------------------------------------------------------------
#
# Stable exit contract so the comparator can land in CI / Harness flows
# without re-inventing semantics:
#
#   0   comparison generated; no binding error
#   2   bad CLI / config / input path (argparse + FileNotFoundError)
#   3   malformed archive or trace, or required manifest-digest binding
#       missing/mismatched when --require-binding-match is set
EXIT_OK = 0
EXIT_BAD_INPUT = 2
EXIT_BAD_EVIDENCE = 3


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--archive",
        type=Path,
        required=True,
        help="Path to Runner archive .tar.gz or extracted directory",
    )
    parser.add_argument(
        "--trace",
        type=Path,
        required=True,
        help="Path to OTLP/JSON trace export",
    )
    parser.add_argument(
        "--out-json",
        type=Path,
        help="Optional path to write the full matrix JSON",
    )
    parser.add_argument(
        "--out-md",
        type=Path,
        help="Optional path to write the Markdown matrix",
    )
    parser.add_argument(
        "--require-binding-match",
        action="store_true",
        help=(
            "Treat any non-`tamper-evident-match` manifest-digest binding as a "
            "hard failure (exit 3). Use this in Arm C dispatches where the "
            "archive and trace must be the same run; leave off for Arm A / "
            "Arm B / unit tests where one side is intentionally absent or "
            "synthetic."
        ),
    )
    args = parser.parse_args(argv)

    if not args.archive.exists():
        sys.stderr.write(f"error: archive path not found: {args.archive}\n")
        return EXIT_BAD_INPUT
    if not args.trace.exists():
        sys.stderr.write(f"error: trace path not found: {args.trace}\n")
        return EXIT_BAD_INPUT

    try:
        runner = parse_runner_archive(args.archive)
        trace = parse_otlp_trace(args.trace)
    except FileNotFoundError as exc:
        sys.stderr.write(f"error: missing archive member: {exc}\n")
        return EXIT_BAD_INPUT
    except json.JSONDecodeError as exc:
        sys.stderr.write(f"error: malformed JSON input: {exc}\n")
        return EXIT_BAD_EVIDENCE
    except (tarfile.TarError, OSError) as exc:
        sys.stderr.write(f"error: archive could not be read: {exc}\n")
        return EXIT_BAD_EVIDENCE

    rows = build_field_matrix(runner, trace)
    matrix = matrix_to_json(rows, runner, trace)

    json_text = json.dumps(matrix, indent=2, sort_keys=True)
    md_text = matrix_to_markdown(rows, matrix["summary"])

    if args.out_json:
        args.out_json.write_text(json_text + "\n", encoding="utf-8")
    if args.out_md:
        args.out_md.write_text(md_text + "\n", encoding="utf-8")

    if not args.out_json and not args.out_md:
        # Default to stdout JSON for piping / CI inspection.
        sys.stdout.write(json_text + "\n")

    binding = matrix["summary"]["manifest_digest_binding"]
    if args.require_binding_match and binding != "tamper-evident-match":
        sys.stderr.write(
            "error: --require-binding-match was set but the trace and archive "
            f"are not bound by a matching manifest digest: {binding}\n"
        )
        return EXIT_BAD_EVIDENCE

    return EXIT_OK


if __name__ == "__main__":
    sys.exit(main())
