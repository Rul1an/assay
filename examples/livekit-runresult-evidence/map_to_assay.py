"""Map a frozen LiveKit testing-result artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any, Optional


PLACEHOLDER_EVENT_TYPE = "example.placeholder.livekit-testing-run-result"
PLACEHOLDER_SOURCE = "urn:example:assay:external:livekit:testing-run-result"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "livekit.testing-run-result.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "runtime_mode",
    "task_label",
    "timestamp",
    "outcome",
    "events",
)
OPTIONAL_TOP_LEVEL_KEYS = {
    "final_output_ref",
    "agent_ref",
    "error_label",
    "sdk_version_ref",
}
TOP_LEVEL_KEYS = set(REQUIRED_KEYS) | OPTIONAL_TOP_LEVEL_KEYS
ALLOWED_OUTCOMES = {"completed", "failed"}
ALLOWED_EVENT_TYPES = {
    "message",
    "function_call",
    "function_call_output",
    "agent_handoff",
}
MAX_MESSAGE_CONTENT_LENGTH = 280
MESSAGE_KEYS = {"type", "role", "content"}
FUNCTION_CALL_KEYS = {"type", "name", "arguments_ref"}
FUNCTION_CALL_OUTPUT_KEYS = {"type", "name", "status"}
AGENT_HANDOFF_KEYS = {"type", "new_agent"}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one LiveKit testing-result artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="LiveKit artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-livekit-<stem>.",
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
    # This sample keeps the fixture corpus inside the same deterministic,
    # integer-valued JSON subset the other interop samples use. It is not a full
    # RFC 8785 / JCS implementation for arbitrary JSON inputs.
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


def _parse_rfc3339_utc(value: Optional[str]) -> str:
    if value is None:
        return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")

    normalized = value.replace("Z", "+00:00")
    try:
        parsed = datetime.fromisoformat(normalized)
    except ValueError as exc:
        raise ValueError(f"invalid RFC3339 timestamp: {value}") from exc
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def _validate_non_empty_string(value: Any, line_label: str, field_name: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{line_label}: {field_name} must be a non-empty string")
    return value


def _validate_short_ref(value: Any, line_label: str, field_name: str) -> str:
    return _validate_non_empty_string(value, line_label, field_name)


def _validate_error_label(value: Any, line_label: str) -> str:
    label = _validate_non_empty_string(value, line_label, "error_label")
    allowed = set("abcdefghijklmnopqrstuvwxyz0123456789_-")
    if any(char not in allowed for char in label):
        raise ValueError(
            f"{line_label}: error_label must be a short classifier using lowercase letters, digits, '_' or '-'"
        )
    return label


def _validate_message_content(value: Any, line_label: str) -> str:
    content = _validate_non_empty_string(value, line_label, "content")
    if len(content) > MAX_MESSAGE_CONTENT_LENGTH:
        raise ValueError(
            f"{line_label}: content must be a short string of at most {MAX_MESSAGE_CONTENT_LENGTH} characters"
        )
    return content


def _validate_message_event(event: dict[str, Any], line_label: str) -> dict[str, Any]:
    missing = [key for key in ("type", "role", "content") if key not in event]
    if missing:
        raise ValueError(f"{line_label}: missing required keys: {', '.join(missing)}")

    unknown = set(event) - MESSAGE_KEYS
    if unknown:
        raise ValueError(f"{line_label}: unsupported keys: {', '.join(sorted(unknown))}")

    return {
        "type": "message",
        "role": _validate_non_empty_string(event["role"], line_label, "role"),
        "content": _validate_message_content(event["content"], line_label),
    }


def _validate_function_call_event(event: dict[str, Any], line_label: str) -> dict[str, Any]:
    missing = [key for key in ("type", "name") if key not in event]
    if missing:
        raise ValueError(f"{line_label}: missing required keys: {', '.join(missing)}")

    unknown = set(event) - FUNCTION_CALL_KEYS
    if unknown:
        raise ValueError(f"{line_label}: unsupported keys: {', '.join(sorted(unknown))}")

    normalized = {
        "type": "function_call",
        "name": _validate_non_empty_string(event["name"], line_label, "name"),
    }
    if "arguments_ref" in event:
        normalized["arguments_ref"] = _validate_short_ref(event["arguments_ref"], line_label, "arguments_ref")
    return normalized


def _validate_function_call_output_event(event: dict[str, Any], line_label: str) -> dict[str, Any]:
    missing = [key for key in ("type", "name") if key not in event]
    if missing:
        raise ValueError(f"{line_label}: missing required keys: {', '.join(missing)}")

    unknown = set(event) - FUNCTION_CALL_OUTPUT_KEYS
    if unknown:
        raise ValueError(f"{line_label}: unsupported keys: {', '.join(sorted(unknown))}")

    normalized = {
        "type": "function_call_output",
        "name": _validate_non_empty_string(event["name"], line_label, "name"),
    }
    if "status" in event:
        normalized["status"] = _validate_short_ref(event["status"], line_label, "status")
    return normalized


def _validate_agent_handoff_event(event: dict[str, Any], line_label: str) -> dict[str, Any]:
    missing = [key for key in ("type", "new_agent") if key not in event]
    if missing:
        raise ValueError(f"{line_label}: missing required keys: {', '.join(missing)}")

    unknown = set(event) - AGENT_HANDOFF_KEYS
    if unknown:
        raise ValueError(f"{line_label}: unsupported keys: {', '.join(sorted(unknown))}")

    return {
        "type": "agent_handoff",
        "new_agent": _validate_short_ref(event["new_agent"], line_label, "new_agent"),
    }


def _validate_events(value: Any, line_label: str) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        raise ValueError(f"{line_label}: events must be a list")
    if not value:
        raise ValueError(f"{line_label}: events must be a non-empty list")

    normalized_events: list[dict[str, Any]] = []
    for index, event in enumerate(value):
        event_label = f"{line_label}: events[{index}]"
        if not isinstance(event, dict):
            raise ValueError(f"{event_label}: event must be an object")
        if "type" not in event:
            raise ValueError(f"{event_label}: missing required keys: type")

        event_type = _validate_non_empty_string(event["type"], event_label, "type")
        if event_type not in ALLOWED_EVENT_TYPES:
            allowed = ", ".join(sorted(ALLOWED_EVENT_TYPES))
            raise ValueError(f"{event_label}: type must be one of: {allowed}")

        if event_type == "message":
            normalized_events.append(_validate_message_event(event, event_label))
        elif event_type == "function_call":
            normalized_events.append(_validate_function_call_event(event, event_label))
        elif event_type == "function_call_output":
            normalized_events.append(_validate_function_call_output_event(event, event_label))
        elif event_type == "agent_handoff":
            normalized_events.append(_validate_agent_handoff_event(event, event_label))
    return normalized_events


def _normalized_record(record: dict[str, Any]) -> dict[str, Any]:
    if record.get("schema") != EXTERNAL_SCHEMA:
        raise ValueError(f"artifact: expected schema {EXTERNAL_SCHEMA}, got {record.get('schema')}")
    if record.get("framework") != "livekit_agents":
        raise ValueError("artifact: framework must be livekit_agents")
    if record.get("surface") != "voice_testing_run_result":
        raise ValueError("artifact: surface must be voice_testing_run_result")
    if record.get("runtime_mode") != "voice.testing":
        raise ValueError("artifact: runtime_mode must be voice.testing")

    missing = [key for key in REQUIRED_KEYS if key not in record]
    if missing:
        raise ValueError(f"artifact: missing required keys: {', '.join(missing)}")

    normalized = {
        "schema": EXTERNAL_SCHEMA,
        "framework": "livekit_agents",
        "surface": "voice_testing_run_result",
        "runtime_mode": "voice.testing",
        "task_label": _validate_non_empty_string(record["task_label"], "artifact", "task_label"),
        "timestamp": _parse_rfc3339_utc(str(record["timestamp"])),
        "outcome": _validate_non_empty_string(record["outcome"], "artifact", "outcome"),
        "events": _validate_events(record["events"], "artifact"),
    }

    if normalized["outcome"] not in ALLOWED_OUTCOMES:
        allowed = ", ".join(sorted(ALLOWED_OUTCOMES))
        raise ValueError(f"artifact: outcome must be one of: {allowed}")

    if "final_output_ref" in record:
        normalized["final_output_ref"] = _validate_short_ref(record["final_output_ref"], "artifact", "final_output_ref")
    if "agent_ref" in record:
        normalized["agent_ref"] = _validate_short_ref(record["agent_ref"], "artifact", "agent_ref")
    if "sdk_version_ref" in record:
        normalized["sdk_version_ref"] = _validate_short_ref(record["sdk_version_ref"], "artifact", "sdk_version_ref")
    if "error_label" in record:
        normalized["error_label"] = _validate_error_label(record["error_label"], "artifact")

    has_error_label = "error_label" in normalized
    if normalized["outcome"] == "completed" and has_error_label:
        raise ValueError("artifact: completed artifacts must not include error_label")
    if normalized["outcome"] == "failed" and not has_error_label:
        raise ValueError("artifact: failed artifacts must include error_label")

    return normalized


def _build_event(record: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    normalized = _normalized_record(record)
    data = {
        "external_system": "livekit_agents",
        "external_surface": "testing-run-result",
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

    assay_run_id = args.assay_run_id or f"import-livekit-{args.input.stem}"
    event = _build_event(normalized, assay_run_id, import_time)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    try:
        args.output.write_text(_canonical_json(event) + "\n", encoding="utf-8")
    except OSError as exc:
        raise SystemExit(str(exc)) from exc
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
