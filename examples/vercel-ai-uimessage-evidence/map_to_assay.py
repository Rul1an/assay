"""Map a frozen Vercel AI SDK UIMessage-derived wrapper artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any, Optional


PLACEHOLDER_EVENT_TYPE = "example.placeholder.vercel-ai-uimessage"
PLACEHOLDER_SOURCE = "urn:example:assay:external:vercel-ai:uimessage"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "vercel-ai.uimessage-wrapper.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "timestamp",
    "messages",
)
OPTIONAL_TOP_LEVEL_KEYS = {
    "thread_ref",
    "stream_protocol",
    "sdk_version_ref",
}
TOP_LEVEL_KEYS = set(REQUIRED_KEYS) | OPTIONAL_TOP_LEVEL_KEYS
ALLOWED_STREAM_PROTOCOLS = {"ui-message-stream-v1", "text-stream"}
ALLOWED_MESSAGE_ROLES = {"system", "user", "assistant"}
ALLOWED_TEXT_STATES = {"streaming", "done"}
ALLOWED_TOOL_STATES = {
    "input-streaming",
    "input-available",
    "output-available",
    "output-error",
}
ALLOWED_METADATA_KEYS = {"createdAt", "model", "totalTokens"}
MAX_TEXT_LENGTH = 320
MAX_ERROR_TEXT_LENGTH = 120
MAX_ID_LENGTH = 80
MAX_METADATA_MODEL_LENGTH = 80
MAX_STRUCT_DEPTH = 2
MAX_OBJECT_KEYS = 6
MAX_ARRAY_ITEMS = 6


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one Vercel AI SDK UIMessage-derived wrapper artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="Vercel AI SDK artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-vercel-ai-<stem>.",
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


def _validate_short_id(value: Any, line_label: str, field_name: str) -> str:
    text = _validate_non_empty_string(value, line_label, field_name)
    if len(text) > MAX_ID_LENGTH:
        raise ValueError(f"{line_label}: {field_name} must be at most {MAX_ID_LENGTH} characters")
    return text


def _validate_bounded_struct(value: Any, line_label: str, field_name: str, depth: int = 0) -> Any:
    if depth > MAX_STRUCT_DEPTH:
        raise ValueError(f"{line_label}: {field_name} exceeds the allowed nesting depth")
    if value is None or isinstance(value, (bool, int)):
        return value
    if isinstance(value, str):
        if not value.strip():
            raise ValueError(f"{line_label}: {field_name} string values must be non-empty")
        if len(value) > MAX_TEXT_LENGTH:
            raise ValueError(f"{line_label}: {field_name} string values must be short")
        return value
    if isinstance(value, list):
        if len(value) > MAX_ARRAY_ITEMS:
            raise ValueError(f"{line_label}: {field_name} arrays must have at most {MAX_ARRAY_ITEMS} items")
        return [
            _validate_bounded_struct(item, line_label, field_name, depth + 1)
            for item in value
        ]
    if isinstance(value, dict):
        if not value:
            raise ValueError(f"{line_label}: {field_name} objects must not be empty")
        if len(value) > MAX_OBJECT_KEYS:
            raise ValueError(f"{line_label}: {field_name} objects must have at most {MAX_OBJECT_KEYS} keys")
        normalized: dict[str, Any] = {}
        for key, nested in value.items():
            key_text = _validate_non_empty_string(key, line_label, f"{field_name} key")
            if len(key_text) > 40:
                raise ValueError(f"{line_label}: {field_name} keys must be short")
            normalized[key_text] = _validate_bounded_struct(nested, line_label, field_name, depth + 1)
        return normalized
    raise ValueError(f"{line_label}: {field_name} must contain only bounded JSON values")


def _validate_text_part(part: dict[str, Any], line_label: str) -> dict[str, Any]:
    expected = {"type", "text", "state"}
    missing = [key for key in ("type", "text") if key not in part]
    if missing:
        raise ValueError(f"{line_label}: missing required keys: {', '.join(missing)}")
    unknown = set(part) - expected
    if unknown:
        raise ValueError(f"{line_label}: unsupported keys: {', '.join(sorted(unknown))}")

    normalized = {
        "type": "text",
        "text": _validate_non_empty_string(part["text"], line_label, "text"),
    }
    if len(normalized["text"]) > MAX_TEXT_LENGTH:
        raise ValueError(f"{line_label}: text must be at most {MAX_TEXT_LENGTH} characters")
    if "state" in part:
        state = _validate_non_empty_string(part["state"], line_label, "state")
        if state not in ALLOWED_TEXT_STATES:
            allowed = ", ".join(sorted(ALLOWED_TEXT_STATES))
            raise ValueError(f"{line_label}: state must be one of: {allowed}")
        normalized["state"] = state
    return normalized


def _validate_tool_part(part: dict[str, Any], line_label: str) -> dict[str, Any]:
    expected = {"type", "toolCallId", "state", "input", "output", "errorText"}
    missing = [key for key in ("type", "toolCallId", "state", "input") if key not in part]
    if missing:
        raise ValueError(f"{line_label}: missing required keys: {', '.join(missing)}")
    unknown = set(part) - expected
    if unknown:
        raise ValueError(f"{line_label}: unsupported keys: {', '.join(sorted(unknown))}")

    part_type = _validate_non_empty_string(part["type"], line_label, "type")
    if not part_type.startswith("tool-") or len(part_type) <= 5:
        raise ValueError(f"{line_label}: tool parts must use a type starting with 'tool-'")

    state = _validate_non_empty_string(part["state"], line_label, "state")
    if state not in ALLOWED_TOOL_STATES:
        allowed = ", ".join(sorted(ALLOWED_TOOL_STATES))
        raise ValueError(f"{line_label}: state must be one of: {allowed}")

    normalized = {
        "type": part_type,
        "toolCallId": _validate_short_id(part["toolCallId"], line_label, "toolCallId"),
        "state": state,
        "input": _validate_bounded_struct(part["input"], line_label, "input"),
    }

    if state == "output-available":
        if "output" not in part:
            raise ValueError(f"{line_label}: output-available parts must include output")
        if "errorText" in part:
            raise ValueError(f"{line_label}: output-available parts must not include errorText")
        normalized["output"] = _validate_bounded_struct(part["output"], line_label, "output")
    elif state == "output-error":
        if "errorText" not in part:
            raise ValueError(f"{line_label}: output-error parts must include errorText")
        if "output" in part:
            raise ValueError(f"{line_label}: output-error parts must not include output")
        error_text = _validate_non_empty_string(part["errorText"], line_label, "errorText")
        if len(error_text) > MAX_ERROR_TEXT_LENGTH:
            raise ValueError(f"{line_label}: errorText must be at most {MAX_ERROR_TEXT_LENGTH} characters")
        normalized["errorText"] = error_text
    elif "output" in part or "errorText" in part:
        raise ValueError(f"{line_label}: {state} parts must not include output or errorText")

    return normalized


def _validate_message_metadata(value: Any, line_label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError(f"{line_label}: metadata must be an object")
    if not value:
        raise ValueError(f"{line_label}: metadata must not be empty")
    unknown = set(value) - ALLOWED_METADATA_KEYS
    if unknown:
        raise ValueError(f"{line_label}: unsupported metadata keys: {', '.join(sorted(unknown))}")

    normalized: dict[str, Any] = {}
    if "createdAt" in value:
        created = value["createdAt"]
        if not isinstance(created, int) or isinstance(created, bool) or created < 0:
            raise ValueError(f"{line_label}: metadata.createdAt must be a non-negative integer")
        normalized["createdAt"] = created
    if "model" in value:
        model = _validate_non_empty_string(value["model"], line_label, "metadata.model")
        if len(model) > MAX_METADATA_MODEL_LENGTH:
            raise ValueError(f"{line_label}: metadata.model must be short")
        normalized["model"] = model
    if "totalTokens" in value:
        tokens = value["totalTokens"]
        if not isinstance(tokens, int) or isinstance(tokens, bool) or tokens < 0:
            raise ValueError(f"{line_label}: metadata.totalTokens must be a non-negative integer")
        normalized["totalTokens"] = tokens
    return normalized


def _validate_messages(value: Any, line_label: str) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        raise ValueError(f"{line_label}: messages must be a list")
    if not value:
        raise ValueError(f"{line_label}: messages must be a non-empty list")

    normalized_messages: list[dict[str, Any]] = []
    for index, message in enumerate(value):
        message_label = f"{line_label}: messages[{index}]"
        if not isinstance(message, dict):
            raise ValueError(f"{message_label}: message must be an object")

        missing = [key for key in ("id", "role", "parts") if key not in message]
        if missing:
            raise ValueError(f"{message_label}: missing required keys: {', '.join(missing)}")

        unknown = set(message) - {"id", "role", "parts", "metadata"}
        if unknown:
            raise ValueError(f"{message_label}: unsupported keys: {', '.join(sorted(unknown))}")

        role = _validate_non_empty_string(message["role"], message_label, "role")
        if role not in ALLOWED_MESSAGE_ROLES:
            allowed = ", ".join(sorted(ALLOWED_MESSAGE_ROLES))
            raise ValueError(f"{message_label}: role must be one of: {allowed}")

        parts = message["parts"]
        if not isinstance(parts, list) or not parts:
            raise ValueError(f"{message_label}: parts must be a non-empty list")

        normalized_parts: list[dict[str, Any]] = []
        for part_index, part in enumerate(parts):
            part_label = f"{message_label}: parts[{part_index}]"
            if not isinstance(part, dict):
                raise ValueError(f"{part_label}: part must be an object")
            if "type" not in part:
                raise ValueError(f"{part_label}: missing required keys: type")

            part_type = _validate_non_empty_string(part["type"], part_label, "type")
            if part_type == "text":
                normalized_parts.append(_validate_text_part(part, part_label))
            elif part_type.startswith("tool-"):
                normalized_parts.append(_validate_tool_part(part, part_label))
            else:
                raise ValueError(f"{part_label}: unsupported part type: {part_type}")

        normalized_message = {
            "id": _validate_short_id(message["id"], message_label, "id"),
            "role": role,
            "parts": normalized_parts,
        }
        if "metadata" in message:
            normalized_message["metadata"] = _validate_message_metadata(message["metadata"], message_label)
        normalized_messages.append(normalized_message)

    return normalized_messages


def _normalized_record(record: dict[str, Any]) -> dict[str, Any]:
    missing = [key for key in REQUIRED_KEYS if key not in record]
    if missing:
        joined = ", ".join(missing)
        raise ValueError(f"artifact: missing required keys: {joined}")

    if record["schema"] != EXTERNAL_SCHEMA:
        raise ValueError(f"artifact: expected schema {EXTERNAL_SCHEMA}, got {record['schema']}")
    if record["framework"] != "vercel_ai_sdk":
        raise ValueError("artifact: framework must be vercel_ai_sdk")
    if record["surface"] != "ui_message_wrapper":
        raise ValueError("artifact: surface must be ui_message_wrapper")

    normalized = {
        "schema": EXTERNAL_SCHEMA,
        "framework": "vercel_ai_sdk",
        "surface": "ui_message_wrapper",
        "timestamp": _parse_rfc3339_utc(str(record["timestamp"])),
        "messages": _validate_messages(record["messages"], "artifact"),
    }

    if "thread_ref" in record:
        normalized["thread_ref"] = _validate_short_id(record["thread_ref"], "artifact", "thread_ref")
    if "stream_protocol" in record:
        protocol = _validate_non_empty_string(record["stream_protocol"], "artifact", "stream_protocol")
        if protocol not in ALLOWED_STREAM_PROTOCOLS:
            allowed = ", ".join(sorted(ALLOWED_STREAM_PROTOCOLS))
            raise ValueError(f"artifact: stream_protocol must be one of: {allowed}")
        normalized["stream_protocol"] = protocol
    if "sdk_version_ref" in record:
        normalized["sdk_version_ref"] = _validate_short_id(record["sdk_version_ref"], "artifact", "sdk_version_ref")

    # Explicitly ignore unknown top-level extras so the sample stays tolerant to
    # future UIMessage-level growth while keeping the bounded seam projection small.
    return normalized


def _build_event(normalized: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    data = {
        "external_system": "vercel_ai_sdk",
        "external_surface": "ui-message-wrapper",
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

    assay_run_id = args.assay_run_id or f"import-vercel-ai-{args.input.stem}"
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
