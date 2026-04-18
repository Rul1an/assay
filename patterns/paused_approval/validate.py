"""Pause-only artifact validation for the paused approval pattern."""

from __future__ import annotations

from typing import Any, Mapping

from ._common import (
    ALLOWED_INTERRUPTION_KEYS,
    ALLOWED_TOP_LEVEL_OPTIONAL_KEYS,
    DEFAULT_SCHEMA,
    DEFAULT_SURFACE,
    FORBIDDEN_TOP_LEVEL_KEY_MESSAGES,
    MAX_INTERRUPTION_COUNT,
    MAX_TOOL_NAME_LENGTH,
    PAUSE_REASON_TOOL_APPROVAL,
    TOLERATED_TOP_LEVEL_EXTENSION_KEYS,
    parse_rfc3339_utc,
    validate_classifier,
    validate_opaque_ref,
)


REQUIRED_TOP_LEVEL_KEYS = (
    "schema",
    "framework",
    "surface",
    "timestamp",
    "pause_reason",
    "interruptions",
    "resume_state_ref",
)
ALLOWED_TOP_LEVEL_KEYS = set(REQUIRED_TOP_LEVEL_KEYS) | ALLOWED_TOP_LEVEL_OPTIONAL_KEYS | TOLERATED_TOP_LEVEL_EXTENSION_KEYS


def _raise_on_forbidden_top_level_keys(artifact: Mapping[str, Any]) -> None:
    present_forbidden = sorted(key for key in artifact if key in FORBIDDEN_TOP_LEVEL_KEY_MESSAGES)
    if not present_forbidden:
        return
    details = "; ".join(FORBIDDEN_TOP_LEVEL_KEY_MESSAGES[key] for key in present_forbidden)
    raise ValueError(
        f"artifact: forbidden top-level keys found: {', '.join(present_forbidden)} ({details})"
    )


def _validate_interruptions(value: Any) -> list[dict[str, Any]]:
    if not isinstance(value, list) or not value:
        raise ValueError("artifact: interruptions must be a non-empty array")
    if len(value) > MAX_INTERRUPTION_COUNT:
        raise ValueError(f"artifact: interruptions must contain at most {MAX_INTERRUPTION_COUNT} items")

    normalized: list[dict[str, Any]] = []
    seen_call_ids: set[str] = set()
    for index, item in enumerate(value):
        line_label = f"artifact: interruptions[{index}]"
        if not isinstance(item, Mapping):
            raise ValueError(f"{line_label} must be an object")
        unknown = set(item) - ALLOWED_INTERRUPTION_KEYS
        if unknown:
            raise ValueError(f"{line_label}: unsupported keys: {', '.join(sorted(unknown))}")
        missing = {"tool_name", "call_id_ref"} - set(item)
        if missing:
            raise ValueError(f"{line_label}: missing required keys: {', '.join(sorted(missing))}")

        call_id_ref = validate_opaque_ref(item["call_id_ref"], line_label, "call_id_ref")
        if call_id_ref in seen_call_ids:
            raise ValueError(f"{line_label}: duplicate call_id_ref: {call_id_ref}")
        seen_call_ids.add(call_id_ref)

        normalized_item = {
            "tool_name": validate_classifier(item["tool_name"], line_label, "tool_name", MAX_TOOL_NAME_LENGTH),
            "call_id_ref": call_id_ref,
        }

        if "agent_ref" in item:
            normalized_item["agent_ref"] = validate_opaque_ref(item["agent_ref"], line_label, "agent_ref")

        if "arguments_hash" in item:
            normalized_item["arguments_hash"] = validate_opaque_ref(
                item["arguments_hash"], line_label, "arguments_hash"
            )

        normalized.append(normalized_item)

    return normalized


def validate_pause_artifact(artifact: Mapping[str, Any]) -> dict[str, Any]:
    """Validate and normalize the canonical pause-only artifact."""

    if not isinstance(artifact, Mapping):
        raise ValueError("artifact: top-level value must be an object")

    _raise_on_forbidden_top_level_keys(artifact)

    unknown = set(artifact) - ALLOWED_TOP_LEVEL_KEYS
    if unknown:
        raise ValueError(f"artifact: unsupported top-level keys: {', '.join(sorted(unknown))}")

    missing = [key for key in REQUIRED_TOP_LEVEL_KEYS if key not in artifact]
    if missing:
        raise ValueError(f"artifact: missing required keys: {', '.join(missing)}")

    normalized = {
        "schema": artifact["schema"] if isinstance(artifact["schema"], str) else None,
        "framework": validate_classifier(artifact["framework"], "artifact", "framework"),
        "surface": validate_classifier(artifact["surface"], "artifact", "surface"),
        "timestamp": parse_rfc3339_utc(artifact["timestamp"]),
        "pause_reason": validate_classifier(artifact["pause_reason"], "artifact", "pause_reason"),
        "interruptions": _validate_interruptions(artifact["interruptions"]),
        "resume_state_ref": validate_opaque_ref(artifact["resume_state_ref"], "artifact", "resume_state_ref"),
    }

    if normalized["schema"] is None or not normalized["schema"].strip():
        raise ValueError("artifact: schema must be a non-empty string")
    normalized["schema"] = normalized["schema"].strip()

    if normalized["pause_reason"] != PAUSE_REASON_TOOL_APPROVAL:
        raise ValueError(f"artifact: pause_reason must be {PAUSE_REASON_TOOL_APPROVAL}")

    for field_name in ("active_agent_ref", "last_agent_ref", "metadata_ref"):
        if field_name in artifact:
            normalized[field_name] = validate_opaque_ref(artifact[field_name], "artifact", field_name)

    if "policy_snapshot_hash" in artifact:
        normalized["policy_snapshot_hash"] = validate_opaque_ref(
            artifact["policy_snapshot_hash"], "artifact", "policy_snapshot_hash"
        )

    if "policy_decisions" in artifact:
        if not isinstance(artifact["policy_decisions"], list) or not artifact["policy_decisions"]:
            raise ValueError("artifact: policy_decisions must be a non-empty array when present")
        normalized["policy_decisions"] = [
            validate_classifier(decision, "artifact", "policy_decisions", 64)
            for decision in artifact["policy_decisions"]
        ]

    return normalized
