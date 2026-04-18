"""Small shared helpers for the paused approval pattern."""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any, Iterable, Mapping


PAUSE_REASON_TOOL_APPROVAL = "tool_approval"
DEFAULT_SCHEMA = "assay.harness.paused-approval.v1"
DEFAULT_SURFACE = "approval_interruption"
MAX_TEXT_LENGTH = 180
MAX_REF_LENGTH = 180
MAX_INTERRUPTION_COUNT = 8
MAX_TOOL_NAME_LENGTH = 64
ALLOWED_TOP_LEVEL_OPTIONAL_KEYS = {
    "active_agent_ref",
    "last_agent_ref",
    "metadata_ref",
}
TOLERATED_TOP_LEVEL_EXTENSION_KEYS = {
    "policy_snapshot_hash",
    "policy_decisions",
}
ALLOWED_INTERRUPTION_KEYS = {
    "tool_name",
    "call_id_ref",
    "agent_ref",
    "arguments_hash",
}
FORBIDDEN_TOP_LEVEL_KEY_MESSAGES = {
    "history": "artifact: history is out of scope for pause-only evidence",
    "session": "artifact: session is out of scope for pause-only evidence",
    "session_id": "artifact: session identifiers are out of scope for pause-only evidence",
    "newItems": "artifact: newItems/new_items is out of scope for pause-only evidence",
    "new_items": "artifact: newItems/new_items is out of scope for pause-only evidence",
    "lastResponseId": "artifact: provider continuation fields are out of scope for pause-only evidence",
    "last_response_id": "artifact: provider continuation fields are out of scope for pause-only evidence",
    "previousResponseId": "artifact: provider continuation fields are out of scope for pause-only evidence",
    "previous_response_id": "artifact: provider continuation fields are out of scope for pause-only evidence",
    "state": "artifact: raw serialized state must not be embedded in the canonical artifact",
    "run_state": "artifact: raw serialized state must not be embedded in the canonical artifact",
    "serialized_state": "artifact: raw serialized state must not be embedded in the canonical artifact",
    "approval_decision": "artifact: resolved approval decision data is out of scope for pause-only evidence",
    "approval_outcome": "artifact: resolved approval decision data is out of scope for pause-only evidence",
    "approved": "artifact: resolved approval decision data is out of scope for pause-only evidence",
    "rejected": "artifact: resolved approval decision data is out of scope for pause-only evidence",
    "reject_reason": "artifact: resolved approval decision data is out of scope for pause-only evidence",
}


def validate_non_empty_string(value: Any, line_label: str, field_name: str, max_length: int) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{line_label}: {field_name} must be a non-empty string")
    normalized = value.strip()
    if len(normalized) > max_length:
        raise ValueError(f"{line_label}: {field_name} must be at most {max_length} characters")
    return normalized


def validate_classifier(value: Any, line_label: str, field_name: str, max_length: int = 80) -> str:
    classifier = validate_non_empty_string(value, line_label, field_name, max_length)
    for char in classifier:
        if not (char.islower() or char.isdigit() or char in {"_", "-"}):
            raise ValueError(
                f"{line_label}: {field_name} must use lowercase letters, digits, '_' or '-' only"
            )
    return classifier


def validate_opaque_ref(value: Any, line_label: str, field_name: str) -> str:
    ref = validate_non_empty_string(value, line_label, field_name, MAX_REF_LENGTH)
    lowered = ref.lower()
    if lowered.startswith("http://") or lowered.startswith("https://") or "://" in lowered:
        raise ValueError(f"{line_label}: {field_name} must be an opaque id, not a URL")
    return ref


def parse_rfc3339_datetime(value: str) -> datetime:
    normalized = value.replace("Z", "+00:00")
    try:
        parsed = datetime.fromisoformat(normalized)
    except ValueError as exc:
        raise ValueError(f"invalid RFC3339 timestamp: {value}") from exc
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def parse_rfc3339_utc(value: str | None) -> str:
    if value is None:
        return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    return parse_rfc3339_datetime(value).isoformat().replace("+00:00", "Z")


def first_present(mapping: Mapping[str, Any], aliases: Iterable[str]) -> Any:
    for alias in aliases:
        if alias in mapping:
            return mapping[alias]
    return None


def nested_present(mapping: Mapping[str, Any], path: tuple[str, ...]) -> Any:
    current: Any = mapping
    for segment in path:
        if not isinstance(current, Mapping) or segment not in current:
            return None
        current = current[segment]
    return current


def reduce_agent_ref(value: Any, line_label: str, field_name: str = "agent_ref") -> str:
    if isinstance(value, str):
        return validate_opaque_ref(value, line_label, field_name)
    if isinstance(value, Mapping):
        direct = first_present(value, ("agent_ref", "agentRef", "ref", "id"))
        if direct is not None:
            return validate_opaque_ref(direct, line_label, field_name)
        name = first_present(value, ("name", "agent_name", "agentName"))
        if name is not None:
            normalized_name = validate_non_empty_string(name, line_label, field_name, MAX_TEXT_LENGTH)
            return f"agent:{normalized_name}"
    raise ValueError(f"{line_label}: {field_name} must be a string or agent-like object")
