#!/usr/bin/env python3
"""Validate and project the Assay-Runner cross-runtime-diff v0 shape.

This is a read-only contract validator and reference projector. It consumes
two normalized runner evidence sets recorded from different runtime fixtures
plus one per-side declared work-dir prefix, and produces the cross-runtime
diff v0 shape frozen in
`docs/reference/runner/cross-runtime-diff-v0.md`.

Without explicit inputs, the script structurally validates the committed
golden `cross-runtime-diff-s5-gemini-v0.json` against the contract. With
`--self-test`, it additionally projects the S5 OpenAI Agents committed
artifacts against inline minimal Gemini fixtures, compares to the committed
golden, and runs a set of property assertions (A1 idempotence, B3 forbidden
keys, C1 no-leak, boundary-aware prefix replacement, runtime identifier
discipline, SDK metadata consistency).

The script never reads raw telemetry, never reads proof packs, never reaches
the network, and never writes outside its declared `--output` path.
"""

from __future__ import annotations

import argparse
import json
import sys
from collections.abc import Iterable
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
GOLDEN_DIR = ROOT / "docs/reference/runner/golden"

OBSERVATION_HEALTH_SCHEMA = "assay.runner.observation_health.v0"
CAPABILITY_SURFACE_SCHEMA = "assay.runner.capability_surface.v0"
CORRELATION_REPORT_SCHEMA = "assay.runner.correlation_report.v0"
SDK_EVENT_SCHEMA = "assay.runner.sdk_event.v0"
CROSS_RUNTIME_DIFF_SCHEMA = "assay.runner.cross_runtime_diff.v0"

KNOWN_RUNTIMES = ("s5_openai_agents", "gemini_google_genai")

SURFACE_CATEGORIES = (
    "filesystem_paths",
    "network_endpoints",
    "process_execs",
    "mcp_tools",
    "policy_decisions",
)

A1_RULE = "work_dir_prefix_only"
OUT_OF_SCOPE_MARKER = "out_of_scope_cross_runtime_v0"
SIDE_BAND_MARKER = "side_band_provenance"
WORK_PLACEHOLDER = "<work>/"

REQUIRED_NON_CLAIMS: tuple[str, ...] = (
    "cross_runtime_no_acceptability_judgment",
    "cross_runtime_no_declared_capability_input",
    "cross_runtime_no_derived_binding_identity",
    "cross_runtime_no_filename_semantic_equivalence",
    "cross_runtime_no_sdk_capability_equivalence",
)

REQUIRED_NOTES: tuple[str, ...] = (
    "cross_runtime_diff_binding_ids_out_of_scope: binding ids are not cross-runtime comparable in v0; required only for within-runtime correlation",
    "cross_runtime_diff_sdk_metadata_side_band: sdk metadata reported as side-band runtime provenance, not capability surface",
    "cross_runtime_diff_work_dir_prefix_canonicalized: filesystem_paths normalized via the A1 work-dir prefix rule",
)

ALLOWED_STATUSES: frozenset[str] = frozenset({
    "clean",
    "partial:health",
    "partial:correlation",
    "partial:unbound",
    "failed",
})

PRECONDITION_KEYS: tuple[str, ...] = (
    "base_health_clean",
    "head_health_clean",
    "base_correlation_clean",
    "head_correlation_clean",
    "stable_tool_call_ids_required",
    "stable_tool_call_ids_present",
    "runtimes_distinct",
)

EXPECTED_SCOPE_V0: dict[str, Any] = {
    "projection": "surface_set",
    "uses_raw_telemetry": False,
    "uses_proof_pack": False,
    "per_binding_capability_values": False,
    "cross_runtime": True,
}

HEALTH_NAME = "observation-health.json"
SURFACE_NAME = "capability-surface.json"
CORRELATION_NAME = "correlation-report.json"
SDK_LAYER_PATH = ("layers", "sdk.ndjson")

DEFAULT_S5_HEALTH = GOLDEN_DIR / "observation-health-openai-agents-kernel-policy-v0.json"
DEFAULT_S5_SURFACE = GOLDEN_DIR / "capability-surface-openai-agents-kernel-policy-v0.json"
DEFAULT_S5_CORRELATION = GOLDEN_DIR / "correlation-report-openai-agents-kernel-policy-v0.json"
EXPECTED_GOLDEN = GOLDEN_DIR / "cross-runtime-diff-s5-gemini-v0.json"


class ValidationError(Exception):
    """Single contract-violation error type."""


# ---------------------------------------------------------------------------
# JSON helpers
# ---------------------------------------------------------------------------


def load_json(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text())
    except OSError as exc:
        raise ValidationError(f"could not read {path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise ValidationError(f"{path} is not valid JSON: {exc}") from exc
    if not isinstance(data, dict):
        raise ValidationError(f"{path} must contain a JSON object")
    return data


def write_json(data: dict[str, Any], output: Path | None) -> None:
    text = json.dumps(data, indent=2) + "\n"
    if output is None:
        print(text, end="")
        return
    output.write_text(text)


def stable(values: Iterable[str]) -> list[str]:
    return sorted(set(values))


# ---------------------------------------------------------------------------
# Primary artifact validation
# ---------------------------------------------------------------------------


def require_schema(data: dict[str, Any], schema: str, label: str) -> None:
    if data.get("schema") != schema:
        raise ValidationError(f"{label} schema must be {schema}")
    run_id = data.get("run_id")
    if not isinstance(run_id, str) or not run_id:
        raise ValidationError(f"{label} run_id must be a non-empty string")


def surface_values(surface: dict[str, Any], category: str) -> set[str]:
    raw = surface.get(category)
    if not isinstance(raw, list) or not all(isinstance(item, str) for item in raw):
        raise ValidationError(f"capability surface {category} must be array[string]")
    return set(raw)


def binding_ids_set(correlation: dict[str, Any]) -> set[str]:
    bindings = correlation.get("bindings")
    if not isinstance(bindings, list):
        raise ValidationError("correlation bindings must be an array")
    ids: set[str] = set()
    for index, item in enumerate(bindings):
        if not isinstance(item, dict):
            raise ValidationError(f"correlation binding {index} must be an object")
        tool_call_id = item.get("tool_call_id")
        if not isinstance(tool_call_id, str) or not tool_call_id:
            raise ValidationError(f"correlation binding {index} lacks stable tool_call_id")
        if tool_call_id in ids:
            raise ValidationError(f"duplicate tool_call_id: {tool_call_id}")
        ids.add(tool_call_id)
    return ids


def health_clean(health: dict[str, Any]) -> bool:
    return (
        health.get("schema") == OBSERVATION_HEALTH_SCHEMA
        and health.get("kernel_layer") == "complete"
        and health.get("ringbuf_drops") == 0
        and health.get("policy_layer") == "present"
        and health.get("sdk_layer") == "self_reported"
        and health.get("cgroup_correlation") == "clean"
    )


def correlation_clean(correlation: dict[str, Any]) -> bool:
    ambiguities = correlation.get("ambiguities")
    return (
        correlation.get("schema") == CORRELATION_REPORT_SCHEMA
        and correlation.get("status") == "clean"
        and isinstance(ambiguities, list)
        and len(ambiguities) == 0
    )


# ---------------------------------------------------------------------------
# SDK side-band provenance (layers/sdk.ndjson)
# ---------------------------------------------------------------------------


def parse_sdk_metadata(lines: Iterable[str], source: str) -> dict[str, str]:
    """Find the unique (sdk_name, sdk_version) pair across SDK events.

    Skips blank lines, skips events with other schema strings, skips events
    that do not carry both sdk_name and sdk_version. If multiple distinct
    pairs appear in the same side, or none, the projector fails per the
    contract's "inconsistent values" rule.
    """
    pairs: set[tuple[str, str]] = set()
    for line_no, raw in enumerate(lines, start=1):
        line = raw.strip()
        if not line:
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError as exc:
            raise ValidationError(f"{source}:{line_no} is not valid JSON: {exc}") from exc
        if not isinstance(event, dict):
            raise ValidationError(f"{source}:{line_no} must be a JSON object")
        if event.get("schema") != SDK_EVENT_SCHEMA:
            continue
        name = event.get("sdk_name")
        version = event.get("sdk_version")
        if name is None or version is None:
            continue
        if not isinstance(name, str) or not name:
            raise ValidationError(f"{source}:{line_no} sdk_name must be a non-empty string")
        if not isinstance(version, str) or not version:
            raise ValidationError(f"{source}:{line_no} sdk_version must be a non-empty string")
        pairs.add((name, version))

    if not pairs:
        raise ValidationError(
            f"{source} has no SDK event carrying both sdk_name and sdk_version"
        )
    if len(pairs) > 1:
        rendered = ", ".join(f"({n!r}, {v!r})" for n, v in sorted(pairs))
        raise ValidationError(
            f"{source} has inconsistent SDK metadata pairs: {rendered}"
        )

    name, version = next(iter(pairs))
    return {"sdk_name": name, "sdk_version": version}


def load_sdk_metadata(path: Path) -> dict[str, str]:
    try:
        text = path.read_text()
    except OSError as exc:
        raise ValidationError(f"could not read {path}: {exc}") from exc
    return parse_sdk_metadata(text.splitlines(), source=str(path))


# ---------------------------------------------------------------------------
# A1 canonicalization
# ---------------------------------------------------------------------------


def normalize_prefix(prefix: str, label: str) -> str:
    if not isinstance(prefix, str) or not prefix:
        raise ValidationError(f"{label} work-dir prefix must be non-empty")
    if not prefix.startswith("/"):
        raise ValidationError(f"{label} work-dir prefix must be an absolute path: {prefix!r}")
    if not prefix.endswith("/"):
        raise ValidationError(f"{label} work-dir prefix must end with '/': {prefix!r}")
    return prefix


def canonicalize_path(path: str, prefix: str) -> str:
    """Apply the A1 work_dir_prefix_only rule.

    Boundary-aware: prefix is required to end with '/', so a prefix of
    '/work/' does NOT match '/workbench/...'. Non-matching paths are
    returned unchanged. The rule is idempotent: P(P(x)) == P(x).
    """
    if path.startswith(prefix):
        return WORK_PLACEHOLDER + path[len(prefix):]
    return path


def canonicalize_paths(paths: Iterable[str], prefix: str) -> set[str]:
    return {canonicalize_path(p, prefix) for p in paths}


# ---------------------------------------------------------------------------
# Set diff per category
# ---------------------------------------------------------------------------


def category_diff(base: set[str], head: set[str]) -> dict[str, list[str]]:
    return {
        "added": stable(head - base),
        "removed": stable(base - head),
        "unchanged": stable(base & head),
    }


# ---------------------------------------------------------------------------
# Projector
# ---------------------------------------------------------------------------


def project_cross_runtime_diff(
    *,
    base_runtime: str,
    head_runtime: str,
    base_work_dir_prefix: str,
    head_work_dir_prefix: str,
    base_health: dict[str, Any],
    base_surface: dict[str, Any],
    base_correlation: dict[str, Any],
    base_sdk: dict[str, str],
    head_health: dict[str, Any],
    head_surface: dict[str, Any],
    head_correlation: dict[str, Any],
    head_sdk: dict[str, str],
) -> dict[str, Any]:
    if base_runtime not in KNOWN_RUNTIMES:
        raise ValidationError(
            f"base_runtime must be one of {KNOWN_RUNTIMES!r}, got {base_runtime!r}"
        )
    if head_runtime not in KNOWN_RUNTIMES:
        raise ValidationError(
            f"head_runtime must be one of {KNOWN_RUNTIMES!r}, got {head_runtime!r}"
        )
    if base_runtime == head_runtime:
        raise ValidationError(
            "base_runtime must differ from head_runtime; same-runtime diffs are "
            "covered by capability-diff-v0"
        )

    base_prefix = normalize_prefix(base_work_dir_prefix, "base")
    head_prefix = normalize_prefix(head_work_dir_prefix, "head")

    require_schema(base_health, OBSERVATION_HEALTH_SCHEMA, "base observation health")
    require_schema(head_health, OBSERVATION_HEALTH_SCHEMA, "head observation health")
    require_schema(base_surface, CAPABILITY_SURFACE_SCHEMA, "base capability surface")
    require_schema(head_surface, CAPABILITY_SURFACE_SCHEMA, "head capability surface")
    require_schema(base_correlation, CORRELATION_REPORT_SCHEMA, "base correlation report")
    require_schema(head_correlation, CORRELATION_REPORT_SCHEMA, "head correlation report")

    base_run_id = str(base_health["run_id"])
    head_run_id = str(head_health["run_id"])
    for label, data, expected in (
        ("base capability surface", base_surface, base_run_id),
        ("base correlation report", base_correlation, base_run_id),
        ("head capability surface", head_surface, head_run_id),
        ("head correlation report", head_correlation, head_run_id),
    ):
        if data.get("run_id") != expected:
            raise ValidationError(
                f"{label} run_id must match the side's observation health run_id"
            )

    base_health_is_clean = health_clean(base_health)
    head_health_is_clean = health_clean(head_health)
    base_correlation_is_clean = correlation_clean(base_correlation)
    head_correlation_is_clean = correlation_clean(head_correlation)

    base_binding_ids = binding_ids_set(base_correlation)
    head_binding_ids = binding_ids_set(head_correlation)

    base_paths = canonicalize_paths(
        surface_values(base_surface, "filesystem_paths"), base_prefix
    )
    head_paths = canonicalize_paths(
        surface_values(head_surface, "filesystem_paths"), head_prefix
    )

    surface_diff: dict[str, dict[str, list[str]]] = {
        "filesystem_paths": category_diff(base_paths, head_paths),
    }
    for category in SURFACE_CATEGORIES[1:]:
        surface_diff[category] = category_diff(
            surface_values(base_surface, category),
            surface_values(head_surface, category),
        )

    preconditions = {
        "base_health_clean": base_health_is_clean,
        "head_health_clean": head_health_is_clean,
        "base_correlation_clean": base_correlation_is_clean,
        "head_correlation_clean": head_correlation_is_clean,
        "stable_tool_call_ids_required": True,
        "stable_tool_call_ids_present": bool(base_binding_ids and head_binding_ids),
        "runtimes_distinct": True,
    }

    if not (base_health_is_clean and head_health_is_clean):
        status = "partial:health"
    elif not (
        base_correlation_is_clean
        and head_correlation_is_clean
        and preconditions["stable_tool_call_ids_present"]
    ):
        status = "partial:correlation"
    else:
        status = "clean"

    return {
        "schema": CROSS_RUNTIME_DIFF_SCHEMA,
        "base_run_id": base_run_id,
        "head_run_id": head_run_id,
        "base_runtime": base_runtime,
        "head_runtime": head_runtime,
        "status": status,
        "preconditions": preconditions,
        "scope": {
            "projection": "surface_set",
            "uses_raw_telemetry": False,
            "uses_proof_pack": False,
            "per_binding_capability_values": False,
            "cross_runtime": True,
        },
        "canonicalization": {
            "filesystem_paths": A1_RULE,
            "network_endpoints": "none",
            "process_execs": "none",
            "mcp_tools": "none",
            "policy_decisions": "none",
        },
        "surface": surface_diff,
        "binding_ids": {"comparison": OUT_OF_SCOPE_MARKER},
        "policy_outcomes": {"comparison": OUT_OF_SCOPE_MARKER},
        "sdk_metadata": {
            "comparison": SIDE_BAND_MARKER,
            "base": dict(base_sdk),
            "head": dict(head_sdk),
        },
        "unbound": {category: [] for category in SURFACE_CATEGORIES},
        "non_claims": list(REQUIRED_NON_CLAIMS),
        "ambiguities": [],
        "notes": list(REQUIRED_NOTES),
    }


# ---------------------------------------------------------------------------
# Contract structural validation (against committed golden, no reprojection)
# ---------------------------------------------------------------------------


def validate_diff_structure(diff: dict[str, Any]) -> None:
    """Validate a v0 cross-runtime diff structurally against the contract.

    This does not reproject; it just enforces the v0 envelope so a committed
    golden cannot drift away from the contract without this check tripping.
    """
    if diff.get("schema") != CROSS_RUNTIME_DIFF_SCHEMA:
        raise ValidationError(f"schema must be {CROSS_RUNTIME_DIFF_SCHEMA}")

    required_top = {
        "schema",
        "base_run_id",
        "head_run_id",
        "base_runtime",
        "head_runtime",
        "status",
        "preconditions",
        "scope",
        "canonicalization",
        "surface",
        "binding_ids",
        "policy_outcomes",
        "sdk_metadata",
        "unbound",
        "non_claims",
        "ambiguities",
        "notes",
    }
    missing = required_top - set(diff)
    extra = set(diff) - required_top
    if missing:
        raise ValidationError(f"diff is missing required top-level fields: {sorted(missing)}")
    if extra:
        raise ValidationError(f"diff has unknown top-level fields: {sorted(extra)}")

    if diff["base_runtime"] not in KNOWN_RUNTIMES:
        raise ValidationError(f"base_runtime not in v0 table: {diff['base_runtime']!r}")
    if diff["head_runtime"] not in KNOWN_RUNTIMES:
        raise ValidationError(f"head_runtime not in v0 table: {diff['head_runtime']!r}")
    if diff["base_runtime"] == diff["head_runtime"]:
        raise ValidationError("base_runtime must differ from head_runtime")

    if diff.get("status") not in ALLOWED_STATUSES:
        raise ValidationError(
            f"status must be one of {sorted(ALLOWED_STATUSES)}, got {diff.get('status')!r}"
        )

    preconditions = diff["preconditions"]
    if not isinstance(preconditions, dict):
        raise ValidationError("preconditions must be an object")
    actual_precondition_keys = set(preconditions)
    expected_precondition_keys = set(PRECONDITION_KEYS)
    missing_pre = expected_precondition_keys - actual_precondition_keys
    extra_pre = actual_precondition_keys - expected_precondition_keys
    if missing_pre:
        raise ValidationError(
            f"preconditions missing required keys: {sorted(missing_pre)}"
        )
    if extra_pre:
        raise ValidationError(
            f"preconditions has unknown keys: {sorted(extra_pre)}"
        )
    for key in PRECONDITION_KEYS:
        value = preconditions[key]
        if not isinstance(value, bool):
            raise ValidationError(
                f"preconditions.{key} must be boolean, got {type(value).__name__}"
            )

    scope = diff["scope"]
    if not isinstance(scope, dict):
        raise ValidationError("scope must be an object")
    if scope != EXPECTED_SCOPE_V0:
        raise ValidationError(
            f"scope must equal v0 expected scope. expected: {EXPECTED_SCOPE_V0}, got: {scope}"
        )

    canonicalization = diff["canonicalization"]
    for category in SURFACE_CATEGORIES:
        if category not in canonicalization:
            raise ValidationError(
                f"canonicalization missing required category: {category}"
            )
    if canonicalization["filesystem_paths"] != A1_RULE:
        raise ValidationError(
            f"canonicalization.filesystem_paths must be {A1_RULE!r}"
        )
    for category in SURFACE_CATEGORIES[1:]:
        if canonicalization[category] != "none":
            raise ValidationError(
                f"canonicalization.{category} must be 'none' in v0"
            )

    surface = diff["surface"]
    for category in SURFACE_CATEGORIES:
        section = surface.get(category)
        if not isinstance(section, dict):
            raise ValidationError(f"surface.{category} must be an object")
        for key in ("added", "removed", "unchanged"):
            if not isinstance(section.get(key), list):
                raise ValidationError(f"surface.{category}.{key} must be an array")
            values = section[key]
            if sorted(values) != values:
                raise ValidationError(
                    f"surface.{category}.{key} must serialize in stable lexicographic order"
                )

    binding_ids = diff["binding_ids"]
    if set(binding_ids) != {"comparison"}:
        raise ValidationError(
            "binding_ids must contain only 'comparison' in v0 cross-runtime (B3)"
        )
    if binding_ids["comparison"] != OUT_OF_SCOPE_MARKER:
        raise ValidationError(
            f"binding_ids.comparison must be {OUT_OF_SCOPE_MARKER!r}"
        )

    policy_outcomes = diff["policy_outcomes"]
    if set(policy_outcomes) != {"comparison"}:
        raise ValidationError(
            "policy_outcomes must contain only 'comparison' in v0 cross-runtime"
        )
    if policy_outcomes["comparison"] != OUT_OF_SCOPE_MARKER:
        raise ValidationError(
            f"policy_outcomes.comparison must be {OUT_OF_SCOPE_MARKER!r}"
        )

    sdk_metadata = diff["sdk_metadata"]
    if set(sdk_metadata) != {"comparison", "base", "head"}:
        raise ValidationError(
            "sdk_metadata must contain only 'comparison', 'base', 'head' in v0"
        )
    if sdk_metadata["comparison"] != SIDE_BAND_MARKER:
        raise ValidationError(
            f"sdk_metadata.comparison must be {SIDE_BAND_MARKER!r}"
        )
    for side in ("base", "head"):
        side_obj = sdk_metadata[side]
        if not isinstance(side_obj, dict):
            raise ValidationError(f"sdk_metadata.{side} must be an object")
        if set(side_obj) != {"sdk_name", "sdk_version"}:
            raise ValidationError(
                f"sdk_metadata.{side} must have exactly sdk_name and sdk_version"
            )
        for key in ("sdk_name", "sdk_version"):
            if not isinstance(side_obj[key], str) or not side_obj[key]:
                raise ValidationError(
                    f"sdk_metadata.{side}.{key} must be a non-empty string"
                )

    unbound = diff["unbound"]
    for category in SURFACE_CATEGORIES:
        if unbound.get(category) != []:
            raise ValidationError(
                f"unbound.{category} must be empty in v0 (no invented per-binding unbinding)"
            )

    if list(diff["non_claims"]) != list(REQUIRED_NON_CLAIMS):
        raise ValidationError(
            "non_claims must equal the five v0 required codes in lexicographic order"
        )
    if list(diff["notes"]) != list(REQUIRED_NOTES):
        raise ValidationError(
            "notes must equal the three v0 required note strings in lexicographic order"
        )
    if diff["ambiguities"] != []:
        raise ValidationError("ambiguities must be empty in v0 clean diffs")

    # Cross-check: SDK metadata values must not appear in surface added/removed/unchanged
    sdk_strings = {sdk_metadata["base"]["sdk_name"], sdk_metadata["base"]["sdk_version"],
                   sdk_metadata["head"]["sdk_name"], sdk_metadata["head"]["sdk_version"]}
    for category in SURFACE_CATEGORIES:
        for key in ("added", "removed", "unchanged"):
            leaked = sdk_strings & set(surface[category][key])
            if leaked:
                raise ValidationError(
                    f"sdk_metadata leaked into surface.{category}.{key}: {sorted(leaked)}"
                )


def validate_clean_diff(diff: dict[str, Any]) -> None:
    """Assert clean-only invariants on top of structural validation.

    Use this when the diff is expected to be `status=clean`: the committed
    v0 golden, the self-test projection result, and any explicit projection
    over evidence that is known to be clean on both sides. For projections
    where partial/failed outcomes are legitimate, call only
    `validate_diff_structure`.
    """
    validate_diff_structure(diff)
    if diff["status"] != "clean":
        raise ValidationError(
            f"clean diff required, got status={diff['status']!r}"
        )
    for key in PRECONDITION_KEYS:
        if diff["preconditions"][key] is not True:
            raise ValidationError(
                f"clean diff requires preconditions.{key}=true, got {diff['preconditions'][key]!r}"
            )


# ---------------------------------------------------------------------------
# Self-test (inline minimal Gemini fixtures + S5 golden + property assertions)
# ---------------------------------------------------------------------------


S5_BASE_RUNTIME = "s5_openai_agents"
GEMINI_HEAD_RUNTIME = "gemini_google_genai"
S5_PREFIX = "/tmp/assay-runner-openai-agents-kernel-policy/work/"
GEMINI_PREFIX = "/tmp/assay-runner-gemini-google-genai-kernel-policy/work/"

S5_SDK_LINES = (
    json.dumps(
        {
            "schema": SDK_EVENT_SCHEMA,
            "run_id": "run_openai_agents_kernel_policy_determinism",
            "seq": 0,
            "event_type": "tool_call_started",
            "sdk_name": "@openai/agents",
            "sdk_version": "0.11.4",
            "tool_call_id": "tc_runner_policy_001",
            "tool": "read_file",
        }
    ),
    json.dumps(
        {
            "schema": SDK_EVENT_SCHEMA,
            "run_id": "run_openai_agents_kernel_policy_determinism",
            "seq": 1,
            "event_type": "tool_call_completed",
            "sdk_name": "@openai/agents",
            "sdk_version": "0.11.4",
            "tool_call_id": "tc_runner_policy_001",
            "tool": "read_file",
        }
    ),
)

GEMINI_HEALTH = {
    "schema": OBSERVATION_HEALTH_SCHEMA,
    "run_id": "run_gemini_google_genai_kernel_policy_determinism",
    "platform": "linux",
    "kernel_layer": "complete",
    "ringbuf_drops": 0,
    "policy_layer": "present",
    "sdk_layer": "self_reported",
    "cgroup_correlation": "clean",
    "notes": [],
}

GEMINI_SURFACE = {
    "schema": CAPABILITY_SURFACE_SCHEMA,
    "run_id": "run_gemini_google_genai_kernel_policy_determinism",
    "filesystem_paths": [
        GEMINI_PREFIX + "gemini-input.txt",
        GEMINI_PREFIX + "policy-input.txt",
    ],
    "network_endpoints": [],
    "process_execs": [],
    "mcp_tools": ["read_file"],
    "policy_decisions": ["allow:read_file"],
}

GEMINI_CORRELATION = {
    "schema": CORRELATION_REPORT_SCHEMA,
    "run_id": "run_gemini_google_genai_kernel_policy_determinism",
    "status": "clean",
    "bindings": [
        {
            "tool_call_id": "ho0csecf",
            "policy_decision": "allow",
        }
    ],
    "ambiguities": [],
}

GEMINI_SDK_LINES = (
    json.dumps(
        {
            "schema": SDK_EVENT_SCHEMA,
            "run_id": "run_gemini_google_genai_kernel_policy_determinism",
            "seq": 0,
            "event_type": "tool_call_started",
            "sdk_name": "google-genai",
            "sdk_version": "2.6.0",
            "tool_call_id": "ho0csecf",
            "tool": "read_file",
        }
    ),
    json.dumps(
        {
            "schema": SDK_EVENT_SCHEMA,
            "run_id": "run_gemini_google_genai_kernel_policy_determinism",
            "seq": 1,
            "event_type": "tool_call_completed",
            "sdk_name": "google-genai",
            "sdk_version": "2.6.0",
            "tool_call_id": "ho0csecf",
            "tool": "read_file",
        }
    ),
)


def project_s5_gemini_self_test() -> dict[str, Any]:
    """Project the S5 ↔ Gemini diff using committed S5 + inline Gemini."""
    base_health = load_json(DEFAULT_S5_HEALTH)
    base_surface = load_json(DEFAULT_S5_SURFACE)
    base_correlation = load_json(DEFAULT_S5_CORRELATION)
    base_sdk = parse_sdk_metadata(S5_SDK_LINES, source="<inline-s5-sdk>")
    head_sdk = parse_sdk_metadata(GEMINI_SDK_LINES, source="<inline-gemini-sdk>")
    return project_cross_runtime_diff(
        base_runtime=S5_BASE_RUNTIME,
        head_runtime=GEMINI_HEAD_RUNTIME,
        base_work_dir_prefix=S5_PREFIX,
        head_work_dir_prefix=GEMINI_PREFIX,
        base_health=base_health,
        base_surface=base_surface,
        base_correlation=base_correlation,
        base_sdk=base_sdk,
        head_health=GEMINI_HEALTH,
        head_surface=GEMINI_SURFACE,
        head_correlation=GEMINI_CORRELATION,
        head_sdk=head_sdk,
    )


def _expect_raises(label: str, fn) -> None:
    try:
        fn()
    except ValidationError:
        return
    raise ValidationError(f"{label} was expected to raise ValidationError but did not")


def self_test() -> None:
    expected = load_json(EXPECTED_GOLDEN)
    validate_clean_diff(expected)

    actual = project_s5_gemini_self_test()
    validate_clean_diff(actual)

    if actual != expected:
        actual_text = json.dumps(actual, indent=2)
        expected_text = json.dumps(expected, indent=2)
        raise ValidationError(
            "S5 ↔ Gemini projection does not match committed golden\n"
            f"actual:\n{actual_text}\nexpected:\n{expected_text}"
        )

    # Property assertions

    # A1 idempotence: P(P(x)) == P(x)
    for path in (
        S5_PREFIX + "deeply/nested/file.txt",
        S5_PREFIX,
        "/absolute/outside/work.txt",
        "<work>/already-canonical.txt",
        "",
    ):
        once = canonicalize_path(path, S5_PREFIX)
        twice = canonicalize_path(once, S5_PREFIX)
        if once != twice:
            raise ValidationError(
                f"canonicalize_path is not idempotent for {path!r}: {once!r} -> {twice!r}"
            )

    # Boundary-aware: /work/ must not match /workbench/
    if canonicalize_path("/workbench/foo.txt", "/work/") != "/workbench/foo.txt":
        raise ValidationError("canonicalize_path leaked across path boundary (/work/ vs /workbench/)")

    # B3 forbidden keys: binding_ids and policy_outcomes must contain only 'comparison'
    for container in ("binding_ids", "policy_outcomes"):
        for forbidden in ("added", "removed", "changed", "unchanged"):
            if forbidden in actual[container]:
                raise ValidationError(
                    f"B3 violation: {container} contains forbidden key {forbidden!r}"
                )

    # C1 no leak: SDK metadata values cannot appear in surface.*
    sdk_strings = {
        actual["sdk_metadata"]["base"]["sdk_name"],
        actual["sdk_metadata"]["base"]["sdk_version"],
        actual["sdk_metadata"]["head"]["sdk_name"],
        actual["sdk_metadata"]["head"]["sdk_version"],
    }
    for category in SURFACE_CATEGORIES:
        for key in ("added", "removed", "unchanged"):
            leaked = sdk_strings & set(actual["surface"][category][key])
            if leaked:
                raise ValidationError(
                    f"C1 violation: sdk_metadata leaked into surface.{category}.{key}: {sorted(leaked)}"
                )

    # policy_decisions MUST remain in surface (negative test of the reviewer's
    # misunderstanding that B3 also out-of-scopes set-level policy comparison)
    policy_surface = actual["surface"]["policy_decisions"]
    if set(policy_surface) != {"added", "removed", "unchanged"}:
        raise ValidationError(
            "surface.policy_decisions must keep added/removed/unchanged shape; "
            "set-level policy comparison stays in surface in v0"
        )
    if policy_surface["unchanged"] != ["allow:read_file"]:
        raise ValidationError(
            "self-test expected surface.policy_decisions.unchanged == ['allow:read_file']"
        )

    # Runtime identifier discipline
    _expect_raises(
        "same-runtime diff",
        lambda: project_cross_runtime_diff(
            base_runtime=S5_BASE_RUNTIME,
            head_runtime=S5_BASE_RUNTIME,
            base_work_dir_prefix=S5_PREFIX,
            head_work_dir_prefix=S5_PREFIX,
            base_health=load_json(DEFAULT_S5_HEALTH),
            base_surface=load_json(DEFAULT_S5_SURFACE),
            base_correlation=load_json(DEFAULT_S5_CORRELATION),
            base_sdk={"sdk_name": "x", "sdk_version": "1"},
            head_health=load_json(DEFAULT_S5_HEALTH),
            head_surface=load_json(DEFAULT_S5_SURFACE),
            head_correlation=load_json(DEFAULT_S5_CORRELATION),
            head_sdk={"sdk_name": "x", "sdk_version": "1"},
        ),
    )
    _expect_raises(
        "unknown runtime identifier",
        lambda: project_cross_runtime_diff(
            base_runtime="anthropic_direct",
            head_runtime=GEMINI_HEAD_RUNTIME,
            base_work_dir_prefix=S5_PREFIX,
            head_work_dir_prefix=GEMINI_PREFIX,
            base_health=load_json(DEFAULT_S5_HEALTH),
            base_surface=load_json(DEFAULT_S5_SURFACE),
            base_correlation=load_json(DEFAULT_S5_CORRELATION),
            base_sdk={"sdk_name": "x", "sdk_version": "1"},
            head_health=GEMINI_HEALTH,
            head_surface=GEMINI_SURFACE,
            head_correlation=GEMINI_CORRELATION,
            head_sdk={"sdk_name": "y", "sdk_version": "1"},
        ),
    )

    # Prefix discipline
    _expect_raises("empty prefix", lambda: normalize_prefix("", "test"))
    _expect_raises("non-absolute prefix", lambda: normalize_prefix("relative/", "test"))
    _expect_raises("prefix without trailing slash", lambda: normalize_prefix("/abs", "test"))

    # SDK metadata discipline
    _expect_raises(
        "inconsistent sdk metadata",
        lambda: parse_sdk_metadata(
            (
                json.dumps({"schema": SDK_EVENT_SCHEMA, "sdk_name": "a", "sdk_version": "1"}),
                json.dumps({"schema": SDK_EVENT_SCHEMA, "sdk_name": "b", "sdk_version": "2"}),
            ),
            source="<inline-bad>",
        ),
    )
    _expect_raises(
        "no sdk metadata event",
        lambda: parse_sdk_metadata(
            (json.dumps({"schema": "other.schema", "sdk_name": "a"}),),
            source="<inline-empty>",
        ),
    )

    # Drift regression tests: validate_clean_diff must catch load-bearing field drift.
    # Mutate a deep copy of the committed golden and assert each drift is caught.
    def _mutated(mutate) -> dict[str, Any]:
        clone = json.loads(json.dumps(expected))
        mutate(clone)
        return clone

    _expect_raises(
        "status drift to failed",
        lambda: validate_clean_diff(_mutated(lambda d: d.__setitem__("status", "failed"))),
    )
    _expect_raises(
        "status drift to partial:health",
        lambda: validate_clean_diff(_mutated(lambda d: d.__setitem__("status", "partial:health"))),
    )
    _expect_raises(
        "status drift to invented value",
        lambda: validate_clean_diff(_mutated(lambda d: d.__setitem__("status", "eventually_clean"))),
    )
    _expect_raises(
        "preconditions.runtimes_distinct=false drift",
        lambda: validate_clean_diff(
            _mutated(lambda d: d["preconditions"].__setitem__("runtimes_distinct", False))
        ),
    )
    _expect_raises(
        "preconditions extra key drift",
        lambda: validate_clean_diff(
            _mutated(lambda d: d["preconditions"].__setitem__("invented_key", True))
        ),
    )
    _expect_raises(
        "preconditions missing key drift",
        lambda: validate_clean_diff(
            _mutated(lambda d: d["preconditions"].pop("runtimes_distinct"))
        ),
    )
    _expect_raises(
        "scope.uses_raw_telemetry=true drift",
        lambda: validate_clean_diff(
            _mutated(lambda d: d["scope"].__setitem__("uses_raw_telemetry", True))
        ),
    )
    _expect_raises(
        "scope.cross_runtime=false drift",
        lambda: validate_clean_diff(
            _mutated(lambda d: d["scope"].__setitem__("cross_runtime", False))
        ),
    )
    _expect_raises(
        "policy_outcomes.added forbidden key drift",
        lambda: validate_clean_diff(
            _mutated(lambda d: d["policy_outcomes"].__setitem__("added", []))
        ),
    )
    _expect_raises(
        "policy_outcomes.changed forbidden key drift",
        lambda: validate_clean_diff(
            _mutated(lambda d: d["policy_outcomes"].__setitem__("changed", []))
        ),
    )
    _expect_raises(
        "binding_ids.added forbidden key drift",
        lambda: validate_clean_diff(
            _mutated(lambda d: d["binding_ids"].__setitem__("added", []))
        ),
    )


# ---------------------------------------------------------------------------
# Explicit projection mode
# ---------------------------------------------------------------------------


def load_evidence_set(
    *,
    directory: Path | None,
    health_path: Path | None,
    surface_path: Path | None,
    correlation_path: Path | None,
    sdk_layer_path: Path | None,
    label: str,
) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any], dict[str, str]]:
    if directory is not None:
        conflicts = (health_path, surface_path, correlation_path, sdk_layer_path)
        if any(p is not None for p in conflicts):
            raise ValidationError(
                f"{label} must use either --{label}-dir or explicit paths, not both"
            )
        health_path = directory / HEALTH_NAME
        surface_path = directory / SURFACE_NAME
        correlation_path = directory / CORRELATION_NAME
        sdk_layer_path = directory.joinpath(*SDK_LAYER_PATH)

    missing = [
        name
        for name, path in (
            ("health", health_path),
            ("surface", surface_path),
            ("correlation", correlation_path),
            ("sdk-layer", sdk_layer_path),
        )
        if path is None
    ]
    if missing:
        joined = ", ".join(missing)
        raise ValidationError(
            f"{label} evidence is missing required artifact path(s): {joined}"
        )

    assert health_path is not None
    assert surface_path is not None
    assert correlation_path is not None
    assert sdk_layer_path is not None

    return (
        load_json(health_path),
        load_json(surface_path),
        load_json(correlation_path),
        load_sdk_metadata(sdk_layer_path),
    )


def custom_projection(args: argparse.Namespace) -> dict[str, Any] | None:
    path_args = (
        args.base_dir,
        args.head_dir,
        args.base_health,
        args.base_surface,
        args.base_correlation,
        args.base_sdk_layer,
        args.head_health,
        args.head_surface,
        args.head_correlation,
        args.head_sdk_layer,
        args.base_runtime,
        args.head_runtime,
        args.base_work_dir_prefix,
        args.head_work_dir_prefix,
    )
    if not any(path_args):
        return None

    for required, label in (
        (args.base_runtime, "--base-runtime"),
        (args.head_runtime, "--head-runtime"),
        (args.base_work_dir_prefix, "--base-work-dir-prefix"),
        (args.head_work_dir_prefix, "--head-work-dir-prefix"),
    ):
        if not required:
            raise ValidationError(
                f"explicit projection requires {label} (cross-runtime contract)"
            )

    base_health, base_surface, base_correlation, base_sdk = load_evidence_set(
        directory=args.base_dir,
        health_path=args.base_health,
        surface_path=args.base_surface,
        correlation_path=args.base_correlation,
        sdk_layer_path=args.base_sdk_layer,
        label="base",
    )
    head_health, head_surface, head_correlation, head_sdk = load_evidence_set(
        directory=args.head_dir,
        health_path=args.head_health,
        surface_path=args.head_surface,
        correlation_path=args.head_correlation,
        sdk_layer_path=args.head_sdk_layer,
        label="head",
    )

    diff = project_cross_runtime_diff(
        base_runtime=args.base_runtime,
        head_runtime=args.head_runtime,
        base_work_dir_prefix=args.base_work_dir_prefix,
        head_work_dir_prefix=args.head_work_dir_prefix,
        base_health=base_health,
        base_surface=base_surface,
        base_correlation=base_correlation,
        base_sdk=base_sdk,
        head_health=head_health,
        head_surface=head_surface,
        head_correlation=head_correlation,
        head_sdk=head_sdk,
    )
    validate_diff_structure(diff)
    return diff


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="run built-in projection + property assertions against committed golden",
    )
    parser.add_argument(
        "--expected",
        type=Path,
        help="compare the projected diff against this golden JSON (used with explicit inputs)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="write projected diff to this path instead of stdout",
    )
    parser.add_argument("--base-runtime", choices=KNOWN_RUNTIMES)
    parser.add_argument("--head-runtime", choices=KNOWN_RUNTIMES)
    parser.add_argument("--base-work-dir-prefix")
    parser.add_argument("--head-work-dir-prefix")
    parser.add_argument(
        "--base-dir",
        type=Path,
        help=f"directory containing base {HEALTH_NAME}, {SURFACE_NAME}, {CORRELATION_NAME}, and layers/sdk.ndjson",
    )
    parser.add_argument(
        "--head-dir",
        type=Path,
        help=f"directory containing head {HEALTH_NAME}, {SURFACE_NAME}, {CORRELATION_NAME}, and layers/sdk.ndjson",
    )
    parser.add_argument("--base-health", type=Path)
    parser.add_argument("--base-surface", type=Path)
    parser.add_argument("--base-correlation", type=Path)
    parser.add_argument("--base-sdk-layer", type=Path)
    parser.add_argument("--head-health", type=Path)
    parser.add_argument("--head-surface", type=Path)
    parser.add_argument("--head-correlation", type=Path)
    parser.add_argument("--head-sdk-layer", type=Path)
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    try:
        if args.self_test:
            self_test()
            print("cross-runtime-diff self-test ok")
            return 0

        projected = custom_projection(args)
        if projected is not None:
            if args.expected is not None:
                expected = load_json(args.expected)
                validate_diff_structure(expected)
                if projected != expected:
                    actual_text = json.dumps(projected, indent=2)
                    expected_text = json.dumps(expected, indent=2)
                    raise ValidationError(
                        f"projection differs from {args.expected}\n"
                        f"actual:\n{actual_text}\nexpected:\n{expected_text}"
                    )
            write_json(projected, args.output)
            return 0

        if args.output is not None:
            raise ValidationError(
                "--output requires --self-test or explicit base/head inputs"
            )
        validate_clean_diff(load_json(EXPECTED_GOLDEN))
    except ValidationError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print("cross-runtime-diff validation ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
