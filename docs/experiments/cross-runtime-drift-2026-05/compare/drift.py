#!/usr/bin/env python3
"""Cross-runtime drift comparator.

Reads two Runner measured-run archives (one per agent runtime) and
emits per-dimension drift between their capability surfaces.

Scope (v0, Slice 2):
  - filesystem paths touched (capability_surface.filesystem_paths)
  - network endpoints (capability_surface.network_endpoints)
  - process execs (capability_surface.process_execs)
  - SDK tool events (layers/sdk.ndjson — tool names per archive)
  - MCP tool surface (capability_surface.mcp_tools)
  - tool invocation order (SDK events' seq per tool_call_id)

Out of scope (explicit follow-ups, NOT silent gaps):
  - read/write/create/remove classification (requires
    kernel.ndjson parsing — deferred to a v2 comparator)
  - per-path access counts
  - latency / token / cost (not a drift signal)

Classification labels per row:
  task-induced     — full overlap; both runtimes touched the same surface
                     element to satisfy the workload contract.
  provider-induced — non-shared items match a provider-host whitelist
                     (api.openai.com, generativelanguage.googleapis.com,
                     ...). Attributable to the model provider's auth or
                     transport, not the agent framework itself.
  runtime-induced  — non-shared items are not provider hosts and not
                     known fixture paths. Attributable to the runtime's
                     loader, sidecar, or vendored deps.
  inconclusive     — one side has zero data for the dimension and the
                     other has > 0. Cannot tell whether the dimension is
                     genuinely empty in arm X or just not measured there.

Exit codes:
  0 - report generated successfully (drift may or may not exist)
  2 - bad CLI args (path missing, write failure, etc.)
  3 - bad archive input (manifest missing, corrupt JSON, etc.)
"""
from __future__ import annotations

import argparse
import dataclasses
import hashlib
import json
import sys
import tarfile
from pathlib import Path
from typing import Any, Iterable

# ---------------------------------------------------------------------------
# Schema strings (also used to identify fixture archives in tests)
# ---------------------------------------------------------------------------

CAPABILITY_SURFACE_SCHEMA = "assay.runner.capability_surface.v0"
SDK_EVENT_SCHEMA = "assay.runner.sdk_event.v0"
DRIFT_REPORT_SCHEMA = "assay.cross_runtime_drift.v0"

MANIFEST_PATH = "manifest.json"
CAPABILITY_SURFACE_PATH = "capability-surface.json"
OBSERVATION_HEALTH_PATH = "observation-health.json"
CORRELATION_REPORT_PATH = "correlation-report.json"
SDK_LAYER_PATH = "layers/sdk.ndjson"

# Hostnames we treat as model-provider transport / auth endpoints.
# Conservative whitelist; runs that surface anything else default to
# `runtime-induced` (an explicit choice — easier to refine a label than
# to walk back a false provider label).
DEFAULT_PROVIDER_HOSTS = (
    "api.openai.com",
    "auth.openai.com",
    "oauth.openai.com",
    "generativelanguage.googleapis.com",
    "oauth2.googleapis.com",
    "accounts.google.com",
)

CLASSIFICATION_TASK = "task-induced"
CLASSIFICATION_PROVIDER = "provider-induced"
CLASSIFICATION_RUNTIME = "runtime-induced"
CLASSIFICATION_INCONCLUSIVE = "inconclusive"


# ---------------------------------------------------------------------------
# Bad-input signalling. Distinct from rule failures so callers can map to
# exit code 3 instead of conflating with a "no drift" result.
# ---------------------------------------------------------------------------


class BadArchiveError(Exception):
    """Raised when an archive is unreadable: manifest missing, JSON
    corrupt, tar broken, etc. Surfaced as exit code 3."""


# ---------------------------------------------------------------------------
# Archive parsing
# ---------------------------------------------------------------------------


@dataclasses.dataclass
class ArchiveObservation:
    """Normalized view of a Runner archive for the drift comparator."""

    path: str
    run_id: str | None
    runtime_label: str | None
    manifest_digest: str
    capability_surface: dict[str, list[str]]
    sdk_events: list[dict[str, Any]]
    sdk_event_count: int
    sdk_tools: list[str]
    sdk_tool_call_ids: list[str]
    sdk_tool_order: list[str]  # (tool_call_id, tool) sequence per seq order


def _open_archive_member(source: Path, member: str) -> bytes | None:
    if source.is_dir():
        path = source / member
        if not path.exists():
            return None
        try:
            return path.read_bytes()
        except OSError as exc:
            raise BadArchiveError(
                f"{source}!{member}: read failed: {exc}"
            ) from exc
    try:
        with tarfile.open(source, "r:*") as tf:
            try:
                extracted = tf.extractfile(member)
            except KeyError:
                return None
            if extracted is None:
                return None
            return extracted.read()
    except tarfile.TarError as exc:
        raise BadArchiveError(f"{source}: corrupt tar archive: {exc}") from exc
    except OSError as exc:
        # FileNotFoundError, PermissionError, etc. when opening the tarball.
        raise BadArchiveError(f"{source}: cannot open archive: {exc}") from exc


def _parse_json(path_repr: str, data: bytes) -> Any:
    try:
        return json.loads(data.decode("utf-8"))
    except json.JSONDecodeError as exc:
        raise BadArchiveError(f"{path_repr}: invalid JSON: {exc}") from exc
    except UnicodeDecodeError as exc:
        raise BadArchiveError(f"{path_repr}: invalid UTF-8: {exc}") from exc


def parse_archive(source: Path) -> ArchiveObservation:
    """Parse a Runner archive (directory or .tar.gz). Raises
    BadArchiveError if the path does not exist, the manifest is missing,
    capability-surface.json is missing, or any required file is corrupt.

    Note: we read the exact manifest bytes for the digest, never
    re-serialized JSON. The drift comparator does not enforce binding
    against a trace (that's compare.py's job in the runner-vs-otel
    experiment), but it still records the digest for traceability."""

    if not source.exists():
        raise BadArchiveError(f"{source}: archive path does not exist")
    if not (source.is_dir() or source.is_file()):
        raise BadArchiveError(
            f"{source}: archive path is neither a directory nor a file"
        )

    manifest_bytes = _open_archive_member(source, MANIFEST_PATH)
    if manifest_bytes is None:
        raise BadArchiveError(f"{source}: manifest.json not found")
    manifest = _parse_json(f"{source}!{MANIFEST_PATH}", manifest_bytes)
    manifest_digest = "sha256:" + hashlib.sha256(manifest_bytes).hexdigest()
    run_id = manifest.get("run_id") if isinstance(manifest, dict) else None
    # runtime_label is derived from SDK events below (the real Runner
    # archive_manifest.v0 schema does not carry a runtime field; the SDK
    # event `source` is the canonical signal).
    runtime_label: str | None = None

    # capability-surface.json is the primary evidence source for the
    # drift comparator. Missing it is a hard exit-3, not a silent
    # "everything inconclusive" report — otherwise an incomplete
    # Runner capture could ship as a valid drift report in Slice 3.
    capability_bytes = _open_archive_member(source, CAPABILITY_SURFACE_PATH)
    if capability_bytes is None:
        raise BadArchiveError(
            f"{source}: capability-surface.json not found "
            f"(required by the drift comparator)"
        )
    capability = _parse_json(
        f"{source}!{CAPABILITY_SURFACE_PATH}", capability_bytes
    )
    capability_surface: dict[str, list[str]] = {
        "filesystem_paths": [],
        "network_endpoints": [],
        "process_execs": [],
        "mcp_tools": [],
        "policy_decisions": [],
    }
    if isinstance(capability, dict):
        for key in capability_surface:
            value = capability.get(key, [])
            if isinstance(value, list):
                capability_surface[key] = sorted(
                    [str(v) for v in value if isinstance(v, str)]
                )

    sdk_events: list[dict[str, Any]] = []
    sdk_bytes = _open_archive_member(source, SDK_LAYER_PATH)
    if sdk_bytes is not None:
        try:
            text = sdk_bytes.decode("utf-8")
        except UnicodeDecodeError as exc:
            raise BadArchiveError(
                f"{source}!{SDK_LAYER_PATH}: invalid UTF-8: {exc}"
            ) from exc
        for lineno, line in enumerate(text.splitlines(), start=1):
            if not line.strip():
                continue
            try:
                sdk_events.append(json.loads(line))
            except json.JSONDecodeError as exc:
                raise BadArchiveError(
                    f"{source}!{SDK_LAYER_PATH}:{lineno}: "
                    f"invalid JSON: {exc}"
                ) from exc

    # Derive runtime label from the first SDK event that carries a
    # `source` field. The Runner SDK-event v0 schema sets source to the
    # workload-side identifier (e.g. "openai-agents", "gemini-genai"),
    # which is the canonical signal. Falls back to None when SDK layer
    # is absent or events do not carry source — the drift report then
    # renders the label as `<unknown>`.
    for event in sdk_events:
        candidate = event.get("source")
        if isinstance(candidate, str) and candidate:
            runtime_label = candidate
            break

    # Derive tool registration / invocation summary from SDK events.
    sdk_tools: list[str] = []
    sdk_tool_call_ids: list[str] = []
    # Ordered (seq) view of tool invocations, useful for invocation-order
    # drift. We treat tool_call_started events as the ordering anchor;
    # tool_call_completed is a closing event for the same id.
    seq_invocations: list[tuple[int, str, str]] = []
    for event in sdk_events:
        tool = event.get("tool")
        if isinstance(tool, str) and tool and tool not in sdk_tools:
            sdk_tools.append(tool)
        call_id = event.get("tool_call_id")
        if (
            isinstance(call_id, str)
            and call_id
            and call_id not in sdk_tool_call_ids
        ):
            sdk_tool_call_ids.append(call_id)
        if event.get("event_type") == "tool_call_started":
            seq = event.get("seq")
            if (
                isinstance(seq, int)
                and isinstance(tool, str)
                and isinstance(call_id, str)
            ):
                seq_invocations.append((seq, call_id, tool))
    seq_invocations.sort(key=lambda t: t[0])
    sdk_tool_order = [f"{call_id}:{tool}" for _, call_id, tool in seq_invocations]

    return ArchiveObservation(
        path=str(source),
        run_id=run_id,
        runtime_label=runtime_label,
        manifest_digest=manifest_digest,
        capability_surface=capability_surface,
        sdk_events=sdk_events,
        sdk_event_count=len(sdk_events),
        sdk_tools=sorted(sdk_tools),
        sdk_tool_call_ids=sorted(sdk_tool_call_ids),
        sdk_tool_order=sdk_tool_order,
    )


# ---------------------------------------------------------------------------
# Drift report model
# ---------------------------------------------------------------------------


@dataclasses.dataclass
class DriftRow:
    """One dimension of the drift report."""

    dimension: str
    source: str  # the archive field/path this row was computed from
    only_in_a: list[str]
    only_in_b: list[str]
    in_both: list[str]
    classification: str  # one of CLASSIFICATION_* above
    detail: str


def _network_endpoint_matches_provider(
    item: str, provider_hosts: tuple[str, ...]
) -> bool:
    """Return True iff `item` parses as a `host:port` network endpoint
    where the host equals (or is a subdomain of) one of the whitelisted
    provider hosts. Substring matching is deliberately avoided so
    filesystem paths containing `api.openai.com` do not get
    misclassified as provider-induced when this function is mistakenly
    called for a non-network dimension."""

    host = item.rsplit(":", 1)[0] if ":" in item else item
    for provider in provider_hosts:
        if host == provider or host.endswith("." + provider):
            return True
    return False


def _classify_row(
    only_in_a: list[str],
    only_in_b: list[str],
    in_both: list[str],
    has_data_a: bool,
    has_data_b: bool,
    provider_hosts: tuple[str, ...],
    fixture_paths: frozenset[str],
    *,
    is_network_dimension: bool = False,
) -> tuple[str, str]:
    """Return (classification, detail) for one drift row.

    `is_network_dimension` gates the provider-host classification path.
    Only the `network_endpoints` row should pass True; for other
    dimensions a filesystem path or tool name that happens to contain a
    provider hostname would otherwise be misclassified as
    `provider-induced` (P2 review feedback)."""

    # Dimension wholly empty on both sides — task didn't exercise it.
    if not has_data_a and not has_data_b:
        return CLASSIFICATION_INCONCLUSIVE, "dimension empty in both arms"

    # One arm has zero data, other has non-zero — cannot attribute.
    if has_data_a != has_data_b:
        absent = "arm-a" if not has_data_a else "arm-b"
        return (
            CLASSIFICATION_INCONCLUSIVE,
            f"dimension absent in {absent}; cannot tell if not emitted "
            f"or not measured",
        )

    # Both arms have data here.
    non_shared = list(only_in_a) + list(only_in_b)
    if not non_shared:
        return CLASSIFICATION_TASK, "full overlap"

    def _is_provider(item: str) -> bool:
        if not is_network_dimension:
            return False
        return _network_endpoint_matches_provider(item, provider_hosts)

    def _is_fixture(item: str) -> bool:
        return item in fixture_paths

    provider_count = sum(1 for x in non_shared if _is_provider(x))
    fixture_count = sum(1 for x in non_shared if _is_fixture(x))
    total = len(non_shared)
    if is_network_dimension and provider_count == total:
        return (
            CLASSIFICATION_PROVIDER,
            f"all {total} non-shared item(s) match provider host whitelist",
        )
    if fixture_count == total:
        return (
            CLASSIFICATION_TASK,
            f"all {total} non-shared item(s) match known fixture paths",
        )
    # Mixed or none-of-the-above → attribute to the runtime.
    detail = f"{total} non-shared item(s); "
    if is_network_dimension:
        detail += (
            f"{provider_count} provider, "
            f"{fixture_count} fixture, "
            f"{total - provider_count - fixture_count} unclassified"
        )
    else:
        detail += (
            f"{fixture_count} fixture, {total - fixture_count} unclassified"
        )
    return CLASSIFICATION_RUNTIME, detail


def _diff_lists(
    a: list[str], b: list[str]
) -> tuple[list[str], list[str], list[str]]:
    """Return (only_in_a, only_in_b, in_both), each sorted."""
    set_a = set(a)
    set_b = set(b)
    return (
        sorted(set_a - set_b),
        sorted(set_b - set_a),
        sorted(set_a & set_b),
    )


def build_drift_report(
    a: ArchiveObservation,
    b: ArchiveObservation,
    *,
    provider_hosts: tuple[str, ...] = DEFAULT_PROVIDER_HOSTS,
    fixture_paths: frozenset[str] = frozenset(),
) -> list[DriftRow]:
    """Compute per-dimension drift rows between two archives.

    `fixture_paths` is the set of paths the workload contract requires
    both runtimes to touch (typically WORKLOAD_INPUT_PATH and
    WORKLOAD_OUTPUT_PATH). Items in non-shared sets that match this set
    are classified as task-induced rather than runtime-induced.
    """

    rows: list[DriftRow] = []

    # --- filesystem paths touched ---
    a_paths = a.capability_surface.get("filesystem_paths", [])
    b_paths = b.capability_surface.get("filesystem_paths", [])
    only_a, only_b, both = _diff_lists(a_paths, b_paths)
    cls, detail = _classify_row(
        only_a,
        only_b,
        both,
        bool(a_paths),
        bool(b_paths),
        provider_hosts,
        fixture_paths,
    )
    rows.append(
        DriftRow(
            dimension="filesystem_paths_touched",
            source="capability_surface.filesystem_paths",
            only_in_a=only_a,
            only_in_b=only_b,
            in_both=both,
            classification=cls,
            detail=detail,
        )
    )

    # --- network endpoints ---
    a_net = a.capability_surface.get("network_endpoints", [])
    b_net = b.capability_surface.get("network_endpoints", [])
    only_a, only_b, both = _diff_lists(a_net, b_net)
    cls, detail = _classify_row(
        only_a,
        only_b,
        both,
        bool(a_net),
        bool(b_net),
        provider_hosts,
        fixture_paths,
        is_network_dimension=True,
    )
    rows.append(
        DriftRow(
            dimension="network_endpoints",
            source="capability_surface.network_endpoints",
            only_in_a=only_a,
            only_in_b=only_b,
            in_both=both,
            classification=cls,
            detail=detail,
        )
    )

    # --- process execs ---
    a_exec = a.capability_surface.get("process_execs", [])
    b_exec = b.capability_surface.get("process_execs", [])
    only_a, only_b, both = _diff_lists(a_exec, b_exec)
    cls, detail = _classify_row(
        only_a,
        only_b,
        both,
        bool(a_exec),
        bool(b_exec),
        provider_hosts,
        fixture_paths,
    )
    rows.append(
        DriftRow(
            dimension="process_execs",
            source="capability_surface.process_execs",
            only_in_a=only_a,
            only_in_b=only_b,
            in_both=both,
            classification=cls,
            detail=detail,
        )
    )

    # --- SDK tool events (tool names invoked per archive) ---
    only_a, only_b, both = _diff_lists(a.sdk_tools, b.sdk_tools)
    cls, detail = _classify_row(
        only_a,
        only_b,
        both,
        bool(a.sdk_tools),
        bool(b.sdk_tools),
        provider_hosts,
        fixture_paths,
    )
    rows.append(
        DriftRow(
            dimension="sdk_tool_events",
            source="layers/sdk.ndjson (tool field, deduplicated)",
            only_in_a=only_a,
            only_in_b=only_b,
            in_both=both,
            classification=cls,
            detail=detail,
        )
    )

    # --- MCP tool surface ---
    a_mcp = a.capability_surface.get("mcp_tools", [])
    b_mcp = b.capability_surface.get("mcp_tools", [])
    only_a, only_b, both = _diff_lists(a_mcp, b_mcp)
    cls, detail = _classify_row(
        only_a,
        only_b,
        both,
        bool(a_mcp),
        bool(b_mcp),
        provider_hosts,
        fixture_paths,
    )
    rows.append(
        DriftRow(
            dimension="mcp_tool_surface",
            source="capability_surface.mcp_tools",
            only_in_a=only_a,
            only_in_b=only_b,
            in_both=both,
            classification=cls,
            detail=detail,
        )
    )

    # --- tool invocation order ---
    # We project each arm's ordered (tool_call_id, tool) into a list of
    # tool names — the *order* is what we diff. If one arm has zero
    # ordered invocations and the other has > 0 → inconclusive. If
    # ordered tool sequences match → task-induced. Otherwise →
    # runtime-induced (ordering is something the runtime controls).
    a_order_tools = [t.split(":", 1)[1] for t in a.sdk_tool_order]
    b_order_tools = [t.split(":", 1)[1] for t in b.sdk_tool_order]
    has_a = bool(a_order_tools)
    has_b = bool(b_order_tools)
    if not has_a and not has_b:
        rows.append(
            DriftRow(
                dimension="tool_invocation_order",
                source="layers/sdk.ndjson (event_type=tool_call_started, "
                "ordered by seq)",
                only_in_a=[],
                only_in_b=[],
                in_both=[],
                classification=CLASSIFICATION_INCONCLUSIVE,
                detail="no tool_call_started events in either arm",
            )
        )
    elif has_a != has_b:
        rows.append(
            DriftRow(
                dimension="tool_invocation_order",
                source="layers/sdk.ndjson (event_type=tool_call_started, "
                "ordered by seq)",
                only_in_a=a_order_tools if has_a else [],
                only_in_b=b_order_tools if has_b else [],
                in_both=[],
                classification=CLASSIFICATION_INCONCLUSIVE,
                detail="invocation order absent in one arm",
            )
        )
    elif a_order_tools == b_order_tools:
        rows.append(
            DriftRow(
                dimension="tool_invocation_order",
                source="layers/sdk.ndjson (event_type=tool_call_started, "
                "ordered by seq)",
                only_in_a=[],
                only_in_b=[],
                in_both=a_order_tools,
                classification=CLASSIFICATION_TASK,
                detail=f"identical sequence: {' -> '.join(a_order_tools)}",
            )
        )
    else:
        rows.append(
            DriftRow(
                dimension="tool_invocation_order",
                source="layers/sdk.ndjson (event_type=tool_call_started, "
                "ordered by seq)",
                only_in_a=a_order_tools,
                only_in_b=b_order_tools,
                in_both=[],
                classification=CLASSIFICATION_RUNTIME,
                detail=(
                    f"a: {' -> '.join(a_order_tools)} | "
                    f"b: {' -> '.join(b_order_tools)}"
                ),
            )
        )

    return rows


# ---------------------------------------------------------------------------
# Output
# ---------------------------------------------------------------------------


def report_to_json(
    a: ArchiveObservation,
    b: ArchiveObservation,
    rows: list[DriftRow],
) -> dict[str, Any]:
    return {
        "schema": DRIFT_REPORT_SCHEMA,
        "archive_a": {
            "path": a.path,
            "run_id": a.run_id,
            "runtime_label": a.runtime_label,
            "manifest_digest": a.manifest_digest,
            "sdk_event_count": a.sdk_event_count,
        },
        "archive_b": {
            "path": b.path,
            "run_id": b.run_id,
            "runtime_label": b.runtime_label,
            "manifest_digest": b.manifest_digest,
            "sdk_event_count": b.sdk_event_count,
        },
        "rows": [dataclasses.asdict(r) for r in rows],
        "summary": {
            "dimensions_checked": len(rows),
            "task_induced": sum(
                1 for r in rows if r.classification == CLASSIFICATION_TASK
            ),
            "provider_induced": sum(
                1 for r in rows if r.classification == CLASSIFICATION_PROVIDER
            ),
            "runtime_induced": sum(
                1 for r in rows if r.classification == CLASSIFICATION_RUNTIME
            ),
            "inconclusive": sum(
                1
                for r in rows
                if r.classification == CLASSIFICATION_INCONCLUSIVE
            ),
        },
    }


def _md_escape_cell(text: str) -> str:
    """Escape a string for inclusion in a Markdown table cell.

    GitHub-flavoured Markdown uses `|` as column separator; an unescaped
    `|` inside a cell ends the cell early and breaks the row. Backslash
    escapes both the pipe and the backslash itself."""
    return text.replace("\\", "\\\\").replace("|", "\\|")


def _fmt_list(items: Iterable[str]) -> str:
    items = list(items)
    if not items:
        return "—"
    return ", ".join(f"`{_md_escape_cell(i)}`" for i in items)


def report_to_md(
    a: ArchiveObservation,
    b: ArchiveObservation,
    rows: list[DriftRow],
) -> str:
    lines: list[str] = []
    lines.append("# Cross-Runtime Drift Report")
    lines.append("")
    lines.append(
        f"- **Arm A:** `{_md_escape_cell(a.runtime_label or '<unknown>')}` "
        f"(`{_md_escape_cell(a.path)}`, "
        f"manifest `{a.manifest_digest}`)"
    )
    lines.append(
        f"- **Arm B:** `{_md_escape_cell(b.runtime_label or '<unknown>')}` "
        f"(`{_md_escape_cell(b.path)}`, "
        f"manifest `{b.manifest_digest}`)"
    )
    lines.append("")
    lines.append(
        "| Dimension | Source | Classification | Only in A | Only in B | "
        "In both | Detail |"
    )
    lines.append("|---|---|---|---|---|---|---|")
    for r in rows:
        lines.append(
            f"| `{_md_escape_cell(r.dimension)}` "
            f"| `{_md_escape_cell(r.source)}` "
            f"| **{_md_escape_cell(r.classification)}** "
            f"| {_fmt_list(r.only_in_a)} "
            f"| {_fmt_list(r.only_in_b)} "
            f"| {_fmt_list(r.in_both)} "
            f"| {_md_escape_cell(r.detail)} |"
        )
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--archive-a", required=True, type=Path, help="Arm A archive (dir or .tar.gz)"
    )
    parser.add_argument(
        "--archive-b", required=True, type=Path, help="Arm B archive (dir or .tar.gz)"
    )
    parser.add_argument(
        "--out-json",
        type=Path,
        help="Write drift.json here. Default: stdout.",
    )
    parser.add_argument(
        "--out-md",
        type=Path,
        help="Write drift.md here. Default: do not write.",
    )
    parser.add_argument(
        "--fixture-path",
        action="append",
        default=[],
        help=(
            "Mark this absolute path as a task-induced fixture so the "
            "comparator does not flag it as runtime-induced. May be "
            "passed multiple times. Typical use: --fixture-path "
            "$WORKLOAD_INPUT_PATH --fixture-path $WORKLOAD_OUTPUT_PATH."
        ),
    )
    parser.add_argument(
        "--provider-host",
        action="append",
        default=[],
        help=(
            "Add a hostname/substring to the provider-endpoint whitelist. "
            "May be passed multiple times. Defaults: "
            + ", ".join(DEFAULT_PROVIDER_HOSTS)
        ),
    )
    args = parser.parse_args(argv)

    try:
        a = parse_archive(args.archive_a)
        b = parse_archive(args.archive_b)
    except BadArchiveError as exc:
        print(f"bad archive: {exc}", file=sys.stderr)
        return 3

    provider_hosts = DEFAULT_PROVIDER_HOSTS + tuple(args.provider_host)
    fixture_paths = frozenset(args.fixture_path)

    rows = build_drift_report(
        a, b, provider_hosts=provider_hosts, fixture_paths=fixture_paths
    )
    payload = report_to_json(a, b, rows)

    if args.out_json:
        try:
            args.out_json.parent.mkdir(parents=True, exist_ok=True)
            args.out_json.write_text(
                json.dumps(payload, indent=2, sort_keys=False) + "\n",
                encoding="utf-8",
            )
        except OSError as exc:
            print(f"failed to write --out-json: {exc}", file=sys.stderr)
            return 2
    else:
        print(json.dumps(payload, indent=2, sort_keys=False))

    if args.out_md:
        try:
            args.out_md.parent.mkdir(parents=True, exist_ok=True)
            args.out_md.write_text(
                report_to_md(a, b, rows), encoding="utf-8"
            )
        except OSError as exc:
            print(f"failed to write --out-md: {exc}", file=sys.stderr)
            return 2

    return 0


if __name__ == "__main__":
    sys.exit(main())
