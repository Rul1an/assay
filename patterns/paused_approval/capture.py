"""Observed pause-state capture helpers for paused approval artifacts."""

from __future__ import annotations

from typing import Any, Mapping

from ._common import (
    MAX_INTERRUPTION_COUNT,
    MAX_TOOL_NAME_LENGTH,
    PAUSE_REASON_TOOL_APPROVAL,
    first_present,
    nested_present,
    parse_rfc3339_utc,
    reduce_agent_ref,
    validate_classifier,
    validate_non_empty_string,
    validate_opaque_ref,
)


INTERRUPTION_LIST_ALIASES = ("interruptions", "approval_items", "pending_approvals")
TIMESTAMP_ALIASES = ("timestamp", "time", "created_at", "createdAt")
ACTIVE_AGENT_ALIASES = ("active_agent_ref", "activeAgentRef", "active_agent", "activeAgent")
LAST_AGENT_ALIASES = ("last_agent_ref", "lastAgentRef", "last_agent", "lastAgent")
METADATA_REF_ALIASES = ("metadata_ref", "metadataRef")
CALL_ID_ALIASES = ("call_id_ref", "call_id", "callId", "tool_call_id", "toolCallId", "tool_use_id", "toolUseId", "id")
TOOL_NAME_ALIASES = ("tool_name", "toolName", "name")
AGENT_REF_ALIASES = ("agent_ref", "agentRef", "agent", "agent_name", "agentName")
ARGUMENTS_HASH_ALIASES = ("arguments_hash", "argumentsHash")


def _extract_call_id(item: Mapping[str, Any], line_label: str) -> str:
    direct = first_present(item, CALL_ID_ALIASES)
    if direct is not None:
        return validate_opaque_ref(direct, line_label, "call_id_ref")

    for path in (("rawItem", "callId"), ("raw_item", "call_id"), ("rawItem", "id"), ("raw_item", "id")):
        nested = nested_present(item, path)
        if nested is not None:
            return validate_opaque_ref(nested, line_label, "call_id_ref")

    raise ValueError(f"{line_label}: call_id_ref could not be derived from known aliases")


def _extract_tool_name(item: Mapping[str, Any], line_label: str) -> str:
    tool_name = first_present(item, TOOL_NAME_ALIASES)
    if tool_name is None:
        raise ValueError(f"{line_label}: tool_name could not be derived from known aliases")
    return validate_classifier(tool_name, line_label, "tool_name", MAX_TOOL_NAME_LENGTH)


def _extract_optional_agent_ref(item: Mapping[str, Any], line_label: str) -> str | None:
    agent_value = first_present(item, AGENT_REF_ALIASES)
    if agent_value is None:
        return None
    return reduce_agent_ref(agent_value, line_label)


def _extract_optional_arguments_hash(item: Mapping[str, Any], line_label: str) -> str | None:
    arguments_hash = first_present(item, ARGUMENTS_HASH_ALIASES)
    if arguments_hash is None:
        return None
    return validate_opaque_ref(arguments_hash, line_label, "arguments_hash")


def _extract_interruptions(payload: Mapping[str, Any]) -> list[dict[str, Any]]:
    raw_interruptions = first_present(payload, INTERRUPTION_LIST_ALIASES)
    if not isinstance(raw_interruptions, list) or not raw_interruptions:
        raise ValueError("capture: interruptions must be a non-empty list")
    if len(raw_interruptions) > MAX_INTERRUPTION_COUNT:
        raise ValueError(f"capture: interruptions must contain at most {MAX_INTERRUPTION_COUNT} items")

    normalized: list[dict[str, Any]] = []
    seen_call_ids: set[str] = set()
    for index, item in enumerate(raw_interruptions):
        line_label = f"capture: interruptions[{index}]"
        if not isinstance(item, Mapping):
            raise ValueError(f"{line_label} must be an object")

        call_id_ref = _extract_call_id(item, line_label)
        if call_id_ref in seen_call_ids:
            raise ValueError(f"{line_label}: duplicate call_id_ref: {call_id_ref}")
        seen_call_ids.add(call_id_ref)

        normalized_item = {
            "tool_name": _extract_tool_name(item, line_label),
            "call_id_ref": call_id_ref,
        }

        agent_ref = _extract_optional_agent_ref(item, line_label)
        if agent_ref is not None:
            normalized_item["agent_ref"] = agent_ref

        arguments_hash = _extract_optional_arguments_hash(item, line_label)
        if arguments_hash is not None:
            normalized_item["arguments_hash"] = arguments_hash

        normalized.append(normalized_item)

    return normalized


def _optional_agent_ref(payload: Mapping[str, Any], aliases: tuple[str, ...], field_name: str) -> str | None:
    value = first_present(payload, aliases)
    if value is None:
        return None
    return reduce_agent_ref(value, "capture", field_name)


def capture_paused_approval(
    payload: Mapping[str, Any],
    *,
    timestamp: str | None = None,
    pause_reason: str = PAUSE_REASON_TOOL_APPROVAL,
    active_agent_ref: str | None = None,
    last_agent_ref: str | None = None,
    metadata_ref: str | None = None,
) -> dict[str, Any]:
    """Capture the observed pause-only shape from a runtime-near payload."""

    if not isinstance(payload, Mapping):
        raise ValueError("capture: payload must be a mapping")

    observed_timestamp = timestamp or first_present(payload, TIMESTAMP_ALIASES)
    if observed_timestamp is None:
        raise ValueError("capture: timestamp could not be derived from known aliases")

    normalized = {
        "timestamp": parse_rfc3339_utc(validate_non_empty_string(observed_timestamp, "capture", "timestamp", 180)),
        "pause_reason": validate_classifier(pause_reason, "capture", "pause_reason", 80),
        "interruptions": _extract_interruptions(payload),
    }

    if normalized["pause_reason"] != PAUSE_REASON_TOOL_APPROVAL:
        raise ValueError("capture: pause_reason must be tool_approval")

    resolved_active_agent = active_agent_ref or _optional_agent_ref(payload, ACTIVE_AGENT_ALIASES, "active_agent_ref")
    if resolved_active_agent is not None:
        normalized["active_agent_ref"] = validate_opaque_ref(
            resolved_active_agent, "capture", "active_agent_ref"
        )

    resolved_last_agent = last_agent_ref or _optional_agent_ref(payload, LAST_AGENT_ALIASES, "last_agent_ref")
    if resolved_last_agent is not None:
        normalized["last_agent_ref"] = validate_opaque_ref(resolved_last_agent, "capture", "last_agent_ref")

    resolved_metadata_ref = metadata_ref or first_present(payload, METADATA_REF_ALIASES)
    if resolved_metadata_ref is not None:
        normalized["metadata_ref"] = validate_opaque_ref(resolved_metadata_ref, "capture", "metadata_ref")

    policy_snapshot_hash = payload.get("policy_snapshot_hash")
    if policy_snapshot_hash is not None:
        normalized["policy_snapshot_hash"] = validate_opaque_ref(
            policy_snapshot_hash, "capture", "policy_snapshot_hash"
        )

    policy_decisions = payload.get("policy_decisions")
    if policy_decisions is not None:
        if not isinstance(policy_decisions, list) or not policy_decisions:
            raise ValueError("capture: policy_decisions must be a non-empty list when present")
        normalized["policy_decisions"] = [
            validate_classifier(decision, "capture", "policy_decisions", 64) for decision in policy_decisions
        ]

    return normalized
