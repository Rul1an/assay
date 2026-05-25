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
  - kernel file operations (layers/kernel.ndjson open metadata)
  - path projection v0 over explicitly declared raw=logical aliases
  - runtime/noise taxonomy v0 metadata for projection classes

Out of scope (explicit follow-ups, NOT silent gaps):
  - unlink/remove classification
  - per-path access counts
  - heuristic path taxonomy (node_modules/cache/provider SDK/etc.)
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
KERNEL_LAYER_PATH = "layers/kernel.ndjson"

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

PATH_PROJECTION_SCHEMA = "assay.runner.path_projection.v0"
RUNTIME_NOISE_TAXONOMY_SCHEMA = "assay.runner.runtime_noise_taxonomy.v0"

CLAIM_RAW_OBSERVED = "raw_observed"
CLAIM_PROJECTED_EQUIVALENT = "projected_equivalent"
CLAIM_INCONCLUSIVE = "inconclusive"

PATH_CLASS_WORKLOAD_FIXTURE = "workload_fixture"
PATH_CLASS_RUNTIME_PACKAGE = "runtime_package"
PATH_CLASS_PROVIDER_SDK = "provider_sdk"
PATH_CLASS_LOADER = "loader"
PATH_CLASS_EXPERIMENT_HARNESS = "experiment_harness"
PATH_CLASS_CACHE = "cache"
NETWORK_CLASS_PROVIDER_API = "provider_api"
NETWORK_CLASS_DNS = "dns"
NETWORK_CLASS_TELEMETRY = "telemetry"
NETWORK_CLASS_PACKAGE_FETCH = "package_fetch"
PATH_CLASS_UNKNOWN = "unknown"

TAXONOMY_CATEGORIES: dict[str, dict[str, Any]] = {
    PATH_CLASS_WORKLOAD_FIXTURE: {
        "applies_to": ["path", "operation"],
        "meaning": "declared workload input, output, or scratch value",
    },
    PATH_CLASS_RUNTIME_PACKAGE: {
        "applies_to": ["path"],
        "meaning": "language runtime or package tree behavior",
    },
    PATH_CLASS_PROVIDER_SDK: {
        "applies_to": ["path", "network", "sdk_event"],
        "meaning": "provider client implementation behavior",
    },
    PATH_CLASS_LOADER: {
        "applies_to": ["path", "process"],
        "meaning": "dynamic loader, interpreter, locale, or bootstrap behavior",
    },
    PATH_CLASS_EXPERIMENT_HARNESS: {
        "applies_to": ["path", "process", "metadata"],
        "meaning": "runner, workflow, comparator, or test harness plumbing",
    },
    PATH_CLASS_CACHE: {
        "applies_to": ["path"],
        "meaning": "package manager, runtime, or tool cache path",
    },
    NETWORK_CLASS_PROVIDER_API: {
        "applies_to": ["network"],
        "meaning": "model/provider API endpoint",
    },
    NETWORK_CLASS_DNS: {
        "applies_to": ["network"],
        "meaning": "DNS lookup endpoint when visible",
    },
    NETWORK_CLASS_TELEMETRY: {
        "applies_to": ["network"],
        "meaning": "observability or SDK telemetry endpoint",
    },
    NETWORK_CLASS_PACKAGE_FETCH: {
        "applies_to": ["network"],
        "meaning": "dependency or package retrieval endpoint",
    },
    PATH_CLASS_UNKNOWN: {
        "applies_to": ["all"],
        "meaning": "observed value did not match a declared taxonomy rule",
    },
}

PROJECTION_NON_CLAIMS = (
    "projection_no_raw_evidence_rewrite",
    "projection_no_semantic_workload_equivalence",
    "projection_no_policy_acceptability_verdict",
    "projection_unknowns_preserved",
    "projection_no_heuristic_noise_taxonomy",
)


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
    kernel_file_operations: list[str] = dataclasses.field(default_factory=list)


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

    kernel_events: list[dict[str, Any]] = []
    kernel_bytes = _open_archive_member(source, KERNEL_LAYER_PATH)
    if kernel_bytes is not None:
        try:
            text = kernel_bytes.decode("utf-8")
        except UnicodeDecodeError as exc:
            raise BadArchiveError(
                f"{source}!{KERNEL_LAYER_PATH}: invalid UTF-8: {exc}"
            ) from exc
        for lineno, line in enumerate(text.splitlines(), start=1):
            if not line.strip():
                continue
            try:
                event = json.loads(line)
            except json.JSONDecodeError as exc:
                raise BadArchiveError(
                    f"{source}!{KERNEL_LAYER_PATH}:{lineno}: "
                    f"invalid JSON: {exc}"
                ) from exc
            if isinstance(event, dict):
                kernel_events.append(event)

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
    kernel_file_operations = _kernel_file_operations(kernel_events)

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
        kernel_file_operations=kernel_file_operations,
    )


def _kernel_file_operations(events: list[dict[str, Any]]) -> list[str]:
    """Project operation-aware open events into stable `op:path` strings.

    The Runner kernel-event v0 shape gained optional `access_mode`,
    `operation_flags`, and `status` fields after the first live baseline.
    Older archives simply produce an empty projection so the comparator
    reports this row as inconclusive instead of pretending touched paths
    can be split into read/write semantics.
    """

    out: set[str] = set()
    for event in events:
        if event.get("kind") != "openat":
            continue
        if event.get("status") not in (None, "success"):
            continue
        path = event.get("value")
        if not isinstance(path, str) or not path:
            continue
        access_mode = event.get("access_mode")
        if isinstance(access_mode, str) and access_mode:
            out.add(f"{access_mode}:{path}")
        operation_flags = event.get("operation_flags")
        if isinstance(operation_flags, list):
            for flag in operation_flags:
                if isinstance(flag, str) and flag:
                    out.add(f"{flag}:{path}")
    return sorted(out)


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
    projection: dict[str, Any] = dataclasses.field(default_factory=dict)


@dataclasses.dataclass(frozen=True)
class PathAlias:
    """A declared raw path -> logical path projection rule.

    The comparator deliberately starts with exact aliases only. This
    keeps projection auditable and prevents path-normalization from
    becoming a hidden rewrite layer.
    """

    raw_path: str
    projected_path: str
    path_class: str = PATH_CLASS_WORKLOAD_FIXTURE
    relation: str = "declared_path"
    rule: str = "declared_path_alias"
    confidence: str = "declared"

    def __post_init__(self) -> None:
        _validate_taxonomy_class(self.path_class, domain="path")


def _taxonomy_payload() -> dict[str, Any]:
    return {
        "schema": RUNTIME_NOISE_TAXONOMY_SCHEMA,
        "status": "vocabulary_only",
        "categories": TAXONOMY_CATEGORIES,
        "non_claims": [
            "taxonomy_no_heuristic_classification",
            "taxonomy_unknowns_preserved",
            "taxonomy_no_policy_verdict",
        ],
    }


def _validate_taxonomy_class(category: str, *, domain: str) -> None:
    spec = TAXONOMY_CATEGORIES.get(category)
    if spec is None:
        allowed = ", ".join(sorted(TAXONOMY_CATEGORIES))
        raise ValueError(
            f"unknown taxonomy class {category!r}; expected one of: {allowed}"
        )
    applies_to = spec.get("applies_to", [])
    if "all" not in applies_to and domain not in applies_to:
        raise ValueError(
            f"taxonomy class {category!r} does not apply to {domain!r}"
        )


@dataclasses.dataclass(frozen=True)
class ProjectedValue:
    raw_value: str
    projected_value: str
    path_class: str
    relation: str
    rule: str
    confidence: str
    claim_level: str


def _parse_path_alias(raw: str) -> PathAlias:
    if "=" not in raw:
        raise ValueError("expected RAW=PROJECTED")
    raw_path, projected_path = raw.split("=", 1)
    raw_path = raw_path.strip()
    projected_path = projected_path.strip()
    if not raw_path or not projected_path:
        raise ValueError("expected non-empty RAW and PROJECTED")
    return PathAlias(raw_path=raw_path, projected_path=projected_path)


def _project_single_path(path: str, aliases: dict[str, PathAlias]) -> ProjectedValue:
    alias = aliases.get(path)
    if alias is None:
        return ProjectedValue(
            raw_value=path,
            projected_value=path,
            path_class=PATH_CLASS_UNKNOWN,
            relation="unmatched",
            rule="no_declared_alias",
            confidence="unknown",
            claim_level=CLAIM_RAW_OBSERVED,
        )
    return ProjectedValue(
        raw_value=path,
        projected_value=alias.projected_path,
        path_class=alias.path_class,
        relation=alias.relation,
        rule=alias.rule,
        confidence=alias.confidence,
        claim_level=CLAIM_PROJECTED_EQUIVALENT,
    )


def _project_path_value(
    value: str, aliases: dict[str, PathAlias]
) -> ProjectedValue:
    """Project either a plain path or an operation-aware `op:path`.

    `kernel_file_operations` values use `read:/path`, `write:/path`,
    etc. The operation prefix is raw evidence from kernel-event open
    metadata, so projection only touches the path suffix.
    """

    if ":" not in value:
        return _project_single_path(value, aliases)
    op, path = value.split(":", 1)
    # Avoid treating URI-ish or host:port values as path operations.
    if "/" not in path:
        return _project_single_path(value, aliases)
    projected = _project_single_path(path, aliases)
    return ProjectedValue(
        raw_value=value,
        projected_value=f"{op}:{projected.projected_value}",
        path_class=projected.path_class,
        relation=projected.relation,
        rule=projected.rule,
        confidence=projected.confidence,
        claim_level=projected.claim_level,
    )


def _projection_payload(
    dimension: str,
    a_values: list[str],
    b_values: list[str],
    aliases: dict[str, PathAlias],
) -> dict[str, Any]:
    if not aliases:
        return {
            "schema": PATH_PROJECTION_SCHEMA,
            "status": "not_applied",
            "reason": "no path aliases declared",
            "taxonomy_schema": RUNTIME_NOISE_TAXONOMY_SCHEMA,
            "non_claims": list(PROJECTION_NON_CLAIMS),
        }

    projected_a = [_project_path_value(value, aliases) for value in a_values]
    projected_b = [_project_path_value(value, aliases) for value in b_values]
    a_projected_values = [item.projected_value for item in projected_a]
    b_projected_values = [item.projected_value for item in projected_b]
    only_a, only_b, both = _diff_lists(a_projected_values, b_projected_values)
    mappings = [
        {"side": "a", **dataclasses.asdict(item)} for item in projected_a
    ] + [{"side": "b", **dataclasses.asdict(item)} for item in projected_b]
    rules = sorted(
        {
            item.rule
            for item in [*projected_a, *projected_b]
            if item.rule != "no_declared_alias"
        }
    )
    has_projected_match = any(
        item.claim_level == CLAIM_PROJECTED_EQUIVALENT
        and item.projected_value in both
        for item in [*projected_a, *projected_b]
    )
    if has_projected_match:
        claim_level = CLAIM_PROJECTED_EQUIVALENT
    else:
        claim_level = CLAIM_INCONCLUSIVE
    return {
        "schema": PATH_PROJECTION_SCHEMA,
        "status": "applied",
        "dimension": dimension,
        "claim_level": claim_level,
        "taxonomy_schema": RUNTIME_NOISE_TAXONOMY_SCHEMA,
        "only_in_a": only_a,
        "only_in_b": only_b,
        "in_both": both,
        "rules": rules,
        "mappings": mappings,
        "non_claims": list(PROJECTION_NON_CLAIMS),
    }


def _path_alias_dict(path_aliases: tuple[PathAlias, ...]) -> dict[str, PathAlias]:
    out: dict[str, PathAlias] = {}
    for alias in path_aliases:
        if alias.raw_path in out:
            raise ValueError(f"duplicate path alias for {alias.raw_path!r}")
        out[alias.raw_path] = alias
    return out


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
    path_aliases: tuple[PathAlias, ...] = (),
) -> list[DriftRow]:
    """Compute per-dimension drift rows between two archives.

    `fixture_paths` is the set of paths the workload contract requires
    both runtimes to touch (typically WORKLOAD_INPUT_PATH and
    WORKLOAD_OUTPUT_PATH). Items in non-shared sets that match this set
    are classified as task-induced rather than runtime-induced.
    """

    rows: list[DriftRow] = []
    aliases = _path_alias_dict(path_aliases)

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
            projection=_projection_payload(
                "filesystem_paths_touched", a_paths, b_paths, aliases
            ),
        )
    )

    # --- operation-aware filesystem opens ---
    only_a, only_b, both = _diff_lists(a.kernel_file_operations, b.kernel_file_operations)
    cls, detail = _classify_row(
        only_a,
        only_b,
        both,
        bool(a.kernel_file_operations),
        bool(b.kernel_file_operations),
        provider_hosts,
        _operation_fixture_paths(fixture_paths),
    )
    rows.append(
        DriftRow(
            dimension="kernel_file_operations",
            source="layers/kernel.ndjson (openat access_mode + operation_flags)",
            only_in_a=only_a,
            only_in_b=only_b,
            in_both=both,
            classification=cls,
            detail=detail,
            projection=_projection_payload(
                "kernel_file_operations",
                a.kernel_file_operations,
                b.kernel_file_operations,
                aliases,
            ),
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


def _operation_fixture_paths(fixture_paths: frozenset[str]) -> frozenset[str]:
    prefixes = ("read", "write", "read_write", "create", "truncate", "append", "exclusive")
    return frozenset(f"{prefix}:{path}" for path in fixture_paths for prefix in prefixes)


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
        "taxonomy": _taxonomy_payload(),
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


def _fmt_projection(projection: dict[str, Any]) -> str:
    if projection.get("status") != "applied":
        return "—"
    claim = str(projection.get("claim_level", CLAIM_INCONCLUSIVE))
    in_both = projection.get("in_both")
    if isinstance(in_both, list) and in_both:
        return f"{_md_escape_cell(claim)}: {_fmt_list(str(i) for i in in_both)}"
    return _md_escape_cell(claim)


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
        "In both | Projection | Detail |"
    )
    lines.append("|---|---|---|---|---|---|---|---|")
    for r in rows:
        lines.append(
            f"| `{_md_escape_cell(r.dimension)}` "
            f"| `{_md_escape_cell(r.source)}` "
            f"| **{_md_escape_cell(r.classification)}** "
            f"| {_fmt_list(r.only_in_a)} "
            f"| {_fmt_list(r.only_in_b)} "
            f"| {_fmt_list(r.in_both)} "
            f"| {_fmt_projection(r.projection)} "
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
        "--path-alias",
        action="append",
        default=[],
        help=(
            "Declare an exact raw-path to logical-path projection as "
            "RAW=PROJECTED. The raw value remains in only_in_a/only_in_b/"
            "in_both; the projection is emitted under row.projection. May "
            "be passed multiple times, e.g. "
            "--path-alias $A_INPUT=workdir/input."
        ),
    )
    parser.add_argument(
        "--provider-host",
        action="append",
        default=[],
        help=(
            "Add a hostname to the provider-endpoint whitelist. "
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
    try:
        path_aliases = tuple(_parse_path_alias(item) for item in args.path_alias)
    except ValueError as exc:
        print(f"bad --path-alias: {exc}", file=sys.stderr)
        return 2
    try:
        _path_alias_dict(path_aliases)
    except ValueError as exc:
        print(f"bad --path-alias: {exc}", file=sys.stderr)
        return 2

    try:
        rows = build_drift_report(
            a,
            b,
            provider_hosts=provider_hosts,
            fixture_paths=fixture_paths,
            path_aliases=path_aliases,
        )
    except ValueError as exc:
        print(f"bad projection config: {exc}", file=sys.stderr)
        return 2
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
