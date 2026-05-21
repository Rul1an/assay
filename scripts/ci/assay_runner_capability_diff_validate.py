#!/usr/bin/env python3
"""Validate and project the Assay-Runner capability-diff v0 shape.

This is a read-only contract validator. It consumes normalized runner golden
artifacts and verifies that the S5 idempotence projection matches the frozen
capability-diff v0 golden JSON. It can also project the same v0 diff shape from
two explicit normalized evidence sets without reading raw telemetry or proof
packs.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
GOLDEN_DIR = ROOT / "docs/reference/runner/golden"

OBSERVATION_HEALTH_SCHEMA = "assay.runner.observation_health.v0"
CAPABILITY_SURFACE_SCHEMA = "assay.runner.capability_surface.v0"
CORRELATION_REPORT_SCHEMA = "assay.runner.correlation_report.v0"
CAPABILITY_DIFF_SCHEMA = "assay.runner.capability_diff.v0"

SURFACE_CATEGORIES = (
    "filesystem_paths",
    "network_endpoints",
    "process_execs",
    "mcp_tools",
    "policy_decisions",
)

DEFAULT_HEALTH = GOLDEN_DIR / "observation-health-openai-agents-kernel-policy-v0.json"
DEFAULT_SURFACE = GOLDEN_DIR / "capability-surface-openai-agents-kernel-policy-v0.json"
DEFAULT_CORRELATION = GOLDEN_DIR / "correlation-report-openai-agents-kernel-policy-v0.json"
DEFAULT_EXPECTED = GOLDEN_DIR / "capability-diff-s5-idempotent-v0.json"

HEALTH_NAME = "observation-health.json"
SURFACE_NAME = "capability-surface.json"
CORRELATION_NAME = "correlation-report.json"


class ValidationError(Exception):
    pass


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


def stable(values: set[str]) -> list[str]:
    return sorted(values)


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


def binding_map(correlation: dict[str, Any]) -> dict[str, str | None]:
    bindings = correlation.get("bindings")
    if not isinstance(bindings, list):
        raise ValidationError("correlation bindings must be an array")
    result: dict[str, str | None] = {}
    for index, item in enumerate(bindings):
        if not isinstance(item, dict):
            raise ValidationError(f"correlation binding {index} must be an object")
        tool_call_id = item.get("tool_call_id")
        if not isinstance(tool_call_id, str) or not tool_call_id:
            raise ValidationError(f"correlation binding {index} lacks stable tool_call_id")
        if tool_call_id in result:
            raise ValidationError(f"duplicate tool_call_id: {tool_call_id}")
        policy_decision = item.get("policy_decision")
        if policy_decision is not None and not isinstance(policy_decision, str):
            raise ValidationError(f"policy_decision for {tool_call_id} must be string or null")
        result[tool_call_id] = policy_decision
    return result


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


def category_diff(base: set[str], head: set[str]) -> dict[str, list[str]]:
    return {
        "added": stable(head - base),
        "removed": stable(base - head),
        "unchanged": stable(base & head),
    }


def project_capability_diff(
    base_health: dict[str, Any],
    base_surface: dict[str, Any],
    base_correlation: dict[str, Any],
    head_health: dict[str, Any],
    head_surface: dict[str, Any],
    head_correlation: dict[str, Any],
) -> dict[str, Any]:
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
            raise ValidationError(f"{label} run_id must match observation health run_id")

    base_health_is_clean = health_clean(base_health)
    head_health_is_clean = health_clean(head_health)
    base_correlation_is_clean = correlation_clean(base_correlation)
    head_correlation_is_clean = correlation_clean(head_correlation)

    base_bindings = binding_map(base_correlation)
    head_bindings = binding_map(head_correlation)
    base_binding_ids = set(base_bindings)
    head_binding_ids = set(head_bindings)
    unchanged_binding_ids = base_binding_ids & head_binding_ids

    surface = {
        category: category_diff(
            surface_values(base_surface, category),
            surface_values(head_surface, category),
        )
        for category in SURFACE_CATEGORIES
    }

    policy_changes = []
    for tool_call_id in sorted(unchanged_binding_ids):
        base_decision = base_bindings[tool_call_id]
        head_decision = head_bindings[tool_call_id]
        if base_decision != head_decision:
            policy_changes.append(
                {
                    "tool_call_id": tool_call_id,
                    "base": base_decision,
                    "head": head_decision,
                }
            )

    preconditions = {
        "base_health_clean": base_health_is_clean,
        "head_health_clean": head_health_is_clean,
        "base_correlation_clean": base_correlation_is_clean,
        "head_correlation_clean": head_correlation_is_clean,
        "stable_tool_call_ids_required": True,
        "stable_tool_call_ids_present": bool(base_binding_ids and head_binding_ids),
    }

    unbound = {category: [] for category in SURFACE_CATEGORIES}
    ambiguities: list[str] = []
    notes: list[str] = []

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

    if (
        status == "clean"
        and all(not surface[category]["added"] and not surface[category]["removed"] for category in SURFACE_CATEGORIES)
        and not (base_binding_ids - head_binding_ids)
        and not (head_binding_ids - base_binding_ids)
        and not policy_changes
    ):
        notes.append("capability_diff_idempotent: base and head evidence sets are identical")

    return {
        "schema": CAPABILITY_DIFF_SCHEMA,
        "base_run_id": base_run_id,
        "head_run_id": head_run_id,
        "status": status,
        "preconditions": preconditions,
        "scope": {
            "projection": "surface_set",
            "uses_raw_telemetry": False,
            "uses_proof_pack": False,
            "per_binding_capability_values": False,
        },
        "surface": surface,
        "binding_ids": {
            "added": stable(head_binding_ids - base_binding_ids),
            "removed": stable(base_binding_ids - head_binding_ids),
            "unchanged": stable(unchanged_binding_ids),
        },
        "policy_outcomes": {
            "changed": policy_changes,
        },
        "unbound": unbound,
        "ambiguities": ambiguities,
        "notes": notes,
    }


def validate_policy_consistency(diff: dict[str, Any]) -> None:
    policy_surface = diff["surface"]["policy_decisions"]
    binding_ids = diff["binding_ids"]
    policy_changes = diff["policy_outcomes"]["changed"]
    if (
        binding_ids["added"] == []
        and binding_ids["removed"] == []
        and (policy_surface["added"] or policy_surface["removed"])
        and not policy_changes
    ):
        raise ValidationError(
            "policy surface changed for stable binding ids without policy_outcomes.changed"
        )


def validate_idempotence(diff: dict[str, Any]) -> None:
    if diff["status"] != "clean":
        raise ValidationError("idempotence diff must be clean")
    for category in SURFACE_CATEGORIES:
        if diff["surface"][category]["added"] or diff["surface"][category]["removed"]:
            raise ValidationError(f"idempotence {category} must not add or remove values")
        if diff["unbound"][category]:
            raise ValidationError(f"idempotence {category} must not contain unbound values")
    if diff["binding_ids"]["added"] or diff["binding_ids"]["removed"]:
        raise ValidationError("idempotence binding ids must not be added or removed")
    if diff["policy_outcomes"]["changed"]:
        raise ValidationError("idempotence policy outcomes must not change")
    if diff["ambiguities"]:
        raise ValidationError("idempotence ambiguities must be empty")
    if diff["notes"] != ["capability_diff_idempotent: base and head evidence sets are identical"]:
        raise ValidationError("idempotence note must use the frozen v0 note code")


def default_idempotence_diff() -> dict[str, Any]:
    health = load_json(DEFAULT_HEALTH)
    surface = load_json(DEFAULT_SURFACE)
    correlation = load_json(DEFAULT_CORRELATION)
    return project_capability_diff(health, surface, correlation, health, surface, correlation)


def load_evidence_set(
    *,
    directory: Path | None,
    health_path: Path | None,
    surface_path: Path | None,
    correlation_path: Path | None,
    label: str,
) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any]]:
    if directory is not None:
        if any(path is not None for path in (health_path, surface_path, correlation_path)):
            raise ValidationError(f"{label} must use either --{label}-dir or explicit paths, not both")
        health_path = directory / HEALTH_NAME
        surface_path = directory / SURFACE_NAME
        correlation_path = directory / CORRELATION_NAME

    missing = [
        name
        for name, path in (
            ("health", health_path),
            ("surface", surface_path),
            ("correlation", correlation_path),
        )
        if path is None
    ]
    if missing:
        joined = ", ".join(missing)
        raise ValidationError(f"{label} evidence is missing required artifact path(s): {joined}")

    assert health_path is not None
    assert surface_path is not None
    assert correlation_path is not None
    return load_json(health_path), load_json(surface_path), load_json(correlation_path)


def custom_projection(args: argparse.Namespace) -> dict[str, Any] | None:
    path_args = (
        args.base_dir,
        args.head_dir,
        args.base_health,
        args.base_surface,
        args.base_correlation,
        args.head_health,
        args.head_surface,
        args.head_correlation,
    )
    if not any(path_args):
        return None

    base_health, base_surface, base_correlation = load_evidence_set(
        directory=args.base_dir,
        health_path=args.base_health,
        surface_path=args.base_surface,
        correlation_path=args.base_correlation,
        label="base",
    )
    head_health, head_surface, head_correlation = load_evidence_set(
        directory=args.head_dir,
        health_path=args.head_health,
        surface_path=args.head_surface,
        correlation_path=args.head_correlation,
        label="head",
    )
    diff = project_capability_diff(
        base_health,
        base_surface,
        base_correlation,
        head_health,
        head_surface,
        head_correlation,
    )
    validate_policy_consistency(diff)
    return diff


def validate_default_golden() -> None:
    actual = default_idempotence_diff()
    expected = load_json(DEFAULT_EXPECTED)
    validate_policy_consistency(actual)
    validate_idempotence(actual)
    if actual != expected:
        actual_text = json.dumps(actual, indent=2, sort_keys=True)
        expected_text = json.dumps(expected, indent=2, sort_keys=True)
        raise ValidationError(
            "capability-diff S5 idempotence golden mismatch\n"
            f"actual:\n{actual_text}\nexpected:\n{expected_text}"
        )


def self_test() -> None:
    validate_default_golden()

    health = load_json(DEFAULT_HEALTH)
    surface = load_json(DEFAULT_SURFACE)
    correlation = load_json(DEFAULT_CORRELATION)

    changed_surface = json.loads(json.dumps(surface))
    changed_surface["policy_decisions"] = ["deny:read_file"]
    changed_correlation = json.loads(json.dumps(correlation))
    changed_correlation["bindings"][0]["policy_decision"] = "deny"
    changed = project_capability_diff(
        health,
        surface,
        correlation,
        health,
        changed_surface,
        changed_correlation,
    )
    validate_policy_consistency(changed)
    if changed["policy_outcomes"]["changed"] != [
        {"tool_call_id": "tc_runner_policy_001", "base": "allow", "head": "deny"}
    ]:
        raise ValidationError("policy outcome change was not projected for stable binding id")

    bad_correlation = json.loads(json.dumps(correlation))
    bad_correlation["bindings"][0]["tool_call_id"] = ""
    try:
        project_capability_diff(health, surface, bad_correlation, health, surface, correlation)
    except ValidationError:
        pass
    else:
        raise ValidationError("missing tool_call_id must fail validation")

    explicit = project_capability_diff(health, surface, correlation, health, surface, correlation)
    if explicit != default_idempotence_diff():
        raise ValidationError("explicit S5 projection must match default idempotence projection")


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run built-in validator tests")
    parser.add_argument(
        "--print",
        action="store_true",
        help="print the projected S5 idempotence diff JSON after validation",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="write projected capability-diff JSON to this path instead of stdout",
    )
    parser.add_argument(
        "--base-dir",
        type=Path,
        help=f"directory containing base {HEALTH_NAME}, {SURFACE_NAME}, and {CORRELATION_NAME}",
    )
    parser.add_argument(
        "--head-dir",
        type=Path,
        help=f"directory containing head {HEALTH_NAME}, {SURFACE_NAME}, and {CORRELATION_NAME}",
    )
    parser.add_argument("--base-health", type=Path, help="base observation-health.json path")
    parser.add_argument("--base-surface", type=Path, help="base capability-surface.json path")
    parser.add_argument("--base-correlation", type=Path, help="base correlation-report.json path")
    parser.add_argument("--head-health", type=Path, help="head observation-health.json path")
    parser.add_argument("--head-surface", type=Path, help="head capability-surface.json path")
    parser.add_argument("--head-correlation", type=Path, help="head correlation-report.json path")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    try:
        projected = custom_projection(args)
        if projected is not None:
            write_json(projected, args.output)
            return 0

        if args.self_test:
            self_test()
        else:
            validate_default_golden()
        if args.print:
            write_json(default_idempotence_diff(), args.output)
            return 0
        if args.output is not None:
            raise ValidationError("--output requires --print or explicit base/head inputs")
    except ValidationError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print("capability-diff validation ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
