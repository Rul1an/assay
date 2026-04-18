"""Map a frozen OpenAI Agents JS approval interruption artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any, Optional


PLACEHOLDER_EVENT_TYPE = "example.placeholder.openai-agents-js-approval-interruption"
PLACEHOLDER_SOURCE = "urn:example:assay:external:openai-agents-js:approval-interruption"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "openai-agents-js.tool-approval-interruption.export.v1"
EXTERNAL_SURFACE = "tool_approval_interruption_resumable_state"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "timestamp",
    "pause_reason",
    "interruptions",
    "resume_state_ref",
)
OPTIONAL_KEYS = {
    "active_agent_ref",
    "last_agent_ref",
    "metadata_ref",
}
TOP_LEVEL_KEYS = set(REQUIRED_KEYS) | OPTIONAL_KEYS
FORBIDDEN_TOP_LEVEL_KEY_MESSAGES = {
    "history": "artifact: history is out of scope for pause-only evidence",
    "session": "artifact: session is out of scope for pause-only evidence",
    "session_id": "artifact: session identifiers are out of scope for pause-only evidence",
    "newItems": "artifact: newItems is out of scope for pause-only evidence",
    "new_items": "artifact: newItems is out of scope for pause-only evidence",
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
ALLOWED_PAUSE_REASONS = {"tool_approval"}
MAX_TEXT_LENGTH = 180
MAX_REF_LENGTH = 120
MAX_INTERRUPTION_COUNT = 8
MAX_TOOL_NAME_LENGTH = 64
ALLOWED_INTERRUPTION_KEYS = {"tool_name", "call_id_ref", "agent_ref"}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one OpenAI Agents JS approval interruption artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="OpenAI Agents JS artifact to read.")
    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="Where to write placeholder Assay NDJSON output.",
    )
    parser.add_argument(
        "--import-time",
        default=None,
        help="RFC3339 UTC timestamp for the Assay envelope time field.",
    )
    parser.add_argument(
        "--assay-run-id",
        default=None,
        help="Optional Assay run id override. Defaults to import-openai-agents-js-<stem>.",
    )
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="Allow overwriting the output file if it already exists.",
    )
    return parser.parse_args()


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"artifact: duplicate JSON key: {key}")
        result[key] = value
    return result


def _normalize_for_hash(value: Any) -> Any:
    if value is None or isinstance(value, (str, int, bool)):
        return value
    if isinstance(value, float):
        if not math.isfinite(value):
            raise ValueError("non-finite floats are not valid in canonical JSON")
        if value.is_integer():
            return int(value)
        raise ValueError("non-integer floats are not valid in this sample's canonical JSON subset")
    if isinstance(value, dict):
        return {str(key): _normalize_for_hash(nested) for key, nested in value.items()}
    if isinstance(value, list):
        return [_normalize_for_hash(item) for item in value]
    if isinstance(value, tuple):
        return [_normalize_for_hash(item) for item in value]
    raise TypeError(f"unsupported canonical JSON value: {type(value).__name__}")


def _canonical_json(value: Any) -> str:
    # This sample keeps the fixture corpus inside the same small deterministic
    # JSON profile the other interop samples use. It is not a full RFC 8785 /
    # JCS implementation for arbitrary JSON inputs.
    normalized = _normalize_for_hash(value)
    return json.dumps(
        normalized,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
        allow_nan=False,
    )


def _sha256(value: Any) -> str:
    return f"sha256:{hashlib.sha256(_canonical_json(value).encode('utf-8')).hexdigest()}"


def _compute_assay_content_hash(data: dict[str, Any]) -> str:
    content_hash_input = {
        "specversion": "1.0",
        "type": PLACEHOLDER_EVENT_TYPE,
        "datacontenttype": "application/json",
        "data": data,
    }
    return _sha256(content_hash_input)


def _parse_rfc3339_datetime(value: str) -> datetime:
    normalized = value.replace("Z", "+00:00")
    try:
        parsed = datetime.fromisoformat(normalized)
    except ValueError as exc:
        raise ValueError(f"invalid RFC3339 timestamp: {value}") from exc
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def _parse_rfc3339_utc(value: Optional[str]) -> str:
    if value is None:
        return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    return _parse_rfc3339_datetime(value).isoformat().replace("+00:00", "Z")


def _validate_non_empty_string(value: Any, line_label: str, field_name: str, max_length: int) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{line_label}: {field_name} must be a non-empty string")
    normalized = value.strip()
    if len(normalized) > max_length:
        raise ValueError(f"{line_label}: {field_name} must be at most {max_length} characters")
    return normalized


def _validate_classifier(value: Any, line_label: str, field_name: str, max_length: int = 80) -> str:
    classifier = _validate_non_empty_string(value, line_label, field_name, max_length)
    for char in classifier:
        if not (char.islower() or char.isdigit() or char in {"_", "-"}):
            raise ValueError(
                f"{line_label}: {field_name} must use lowercase letters, digits, '_' or '-' only"
            )
    return classifier


def _validate_opaque_ref(value: Any, line_label: str, field_name: str) -> str:
    ref = _validate_non_empty_string(value, line_label, field_name, MAX_REF_LENGTH)
    lowered = ref.lower()
    if lowered.startswith("http://") or lowered.startswith("https://") or "://" in lowered:
        raise ValueError(f"{line_label}: {field_name} must be an opaque id, not a URL")
    return ref


def _validate_timestamp(value: Any) -> str:
    return _parse_rfc3339_datetime(
        _validate_non_empty_string(value, "artifact", "timestamp", MAX_TEXT_LENGTH)
    ).isoformat().replace("+00:00", "Z")


def _validate_interruptions(value: Any) -> list[dict[str, Any]]:
    if not isinstance(value, list) or not value:
        raise ValueError("artifact: interruptions must be a non-empty array")
    if len(value) > MAX_INTERRUPTION_COUNT:
        raise ValueError(f"artifact: interruptions must contain at most {MAX_INTERRUPTION_COUNT} items")

    normalized: list[dict[str, Any]] = []
    seen_call_ids: set[str] = set()
    for index, item in enumerate(value):
        line_label = f"artifact: interruptions[{index}]"
        if not isinstance(item, dict):
            raise ValueError(f"{line_label} must be an object")
        unknown = set(item) - ALLOWED_INTERRUPTION_KEYS
        if unknown:
            raise ValueError(f"{line_label}: unsupported keys: {', '.join(sorted(unknown))}")
        missing = {"tool_name", "call_id_ref"} - set(item)
        if missing:
            raise ValueError(f"{line_label}: missing required keys: {', '.join(sorted(missing))}")

        tool_name = _validate_classifier(item["tool_name"], line_label, "tool_name", MAX_TOOL_NAME_LENGTH)
        call_id_ref = _validate_opaque_ref(item["call_id_ref"], line_label, "call_id_ref")
        if call_id_ref in seen_call_ids:
            raise ValueError(f"{line_label}: duplicate call_id_ref: {call_id_ref}")
        seen_call_ids.add(call_id_ref)

        normalized_item = {
            "tool_name": tool_name,
            "call_id_ref": call_id_ref,
        }
        if "agent_ref" in item:
            normalized_item["agent_ref"] = _validate_opaque_ref(item["agent_ref"], line_label, "agent_ref")
        normalized.append(normalized_item)

    return normalized


def _raise_on_forbidden_top_level_keys(record: dict[str, Any]) -> None:
    # This mapper enforces the Assay-side pause-only evidence boundary. It is
    # the reference/validation counterpart of a corresponding Harness capture
    # pattern, so it stays intentionally smaller than full runtime or harness
    # state.
    present_forbidden = sorted(key for key in record if key in FORBIDDEN_TOP_LEVEL_KEY_MESSAGES)
    if not present_forbidden:
        return

    first_key = present_forbidden[0]
    raise ValueError(FORBIDDEN_TOP_LEVEL_KEY_MESSAGES[first_key])


def _normalized_record(record: dict[str, Any]) -> dict[str, Any]:
    _raise_on_forbidden_top_level_keys(record)

    unknown = set(record) - TOP_LEVEL_KEYS
    if unknown:
        raise ValueError(f"artifact: unsupported top-level keys: {', '.join(sorted(unknown))}")

    missing = [key for key in REQUIRED_KEYS if key not in record]
    if missing:
        raise ValueError(f"artifact: missing required keys: {', '.join(missing)}")

    schema = _validate_non_empty_string(record["schema"], "artifact", "schema", MAX_TEXT_LENGTH)
    if schema != EXTERNAL_SCHEMA:
        raise ValueError(f"artifact: schema must be {EXTERNAL_SCHEMA}")

    framework = _validate_classifier(record["framework"], "artifact", "framework")
    if framework != "openai_agents_js":
        raise ValueError("artifact: framework must be openai_agents_js")

    surface = _validate_classifier(record["surface"], "artifact", "surface")
    if surface != EXTERNAL_SURFACE:
        raise ValueError(f"artifact: surface must be {EXTERNAL_SURFACE}")

    pause_reason = _validate_classifier(record["pause_reason"], "artifact", "pause_reason")
    if pause_reason not in ALLOWED_PAUSE_REASONS:
        allowed = ", ".join(sorted(ALLOWED_PAUSE_REASONS))
        raise ValueError(f"artifact: pause_reason must be one of: {allowed}")

    normalized = {
        "schema": schema,
        "framework": framework,
        "surface": surface,
        "timestamp": _validate_timestamp(record["timestamp"]),
        "pause_reason": pause_reason,
        "interruptions": _validate_interruptions(record["interruptions"]),
        "resume_state_ref": _validate_opaque_ref(record["resume_state_ref"], "artifact", "resume_state_ref"),
    }

    if "active_agent_ref" in record:
        normalized["active_agent_ref"] = _validate_opaque_ref(
            record["active_agent_ref"], "artifact", "active_agent_ref"
        )

    if "last_agent_ref" in record:
        normalized["last_agent_ref"] = _validate_opaque_ref(
            record["last_agent_ref"], "artifact", "last_agent_ref"
        )

    if "metadata_ref" in record:
        normalized["metadata_ref"] = _validate_opaque_ref(record["metadata_ref"], "artifact", "metadata_ref")

    return normalized


def _build_event(normalized: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    data = {
        "external_system": "openai-agents-js",
        "external_surface": "tool-approval-interruption-resumable-state",
        "external_schema": EXTERNAL_SCHEMA,
        "observed_upstream_time": normalized["timestamp"],
        "observed": normalized,
    }
    event = {
        "specversion": "1.0",
        "type": PLACEHOLDER_EVENT_TYPE,
        "source": PLACEHOLDER_SOURCE,
        "id": f"{assay_run_id}:0",
        "time": import_time,
        "datacontenttype": "application/json",
        "assayrunid": assay_run_id,
        "assayseq": 0,
        "assayproducer": PLACEHOLDER_PRODUCER,
        "assayproducerversion": PLACEHOLDER_PRODUCER_VERSION,
        "assaygit": PLACEHOLDER_GIT,
        "assaypii": False,
        "assaysecrets": False,
        "data": data,
    }
    event["assaycontenthash"] = _compute_assay_content_hash(data)
    return event


def main() -> int:
    args = _parse_args()
    if args.output.exists() and not args.overwrite:
        raise SystemExit(f"{args.output} already exists; pass --overwrite to replace it")

    try:
        with args.input.open("r", encoding="utf-8") as handle:
            record = json.load(handle, object_pairs_hook=_reject_duplicate_keys)
    except (OSError, json.JSONDecodeError, ValueError) as exc:
        raise SystemExit(str(exc)) from exc

    if not isinstance(record, dict):
        raise SystemExit("artifact: top-level JSON value must be an object")

    try:
        normalized = _normalized_record(record)
        import_time = _parse_rfc3339_utc(args.import_time)
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc

    assay_run_id = args.assay_run_id or f"import-openai-agents-js-{args.input.stem}"
    event = _build_event(normalized, assay_run_id, import_time)

    try:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with args.output.open("w", encoding="utf-8") as handle:
            handle.write(_canonical_json(event))
            handle.write("\n")
    except OSError as exc:
        raise SystemExit(str(exc)) from exc

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
