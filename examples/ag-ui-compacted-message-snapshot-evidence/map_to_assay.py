"""Map a frozen AG-UI compacted message snapshot artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any, Optional


PLACEHOLDER_EVENT_TYPE = "example.placeholder.ag-ui-compacted-message-snapshot"
PLACEHOLDER_SOURCE = "urn:example:assay:external:ag-ui:compacted-message-snapshot"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "ag-ui.compacted-message-snapshot.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "thread_id_ref",
    "run_id_ref",
    "started_at",
    "messages",
    "terminal_event",
)
OPTIONAL_KEYS = {
    "finished_at",
    "parent_run_id_ref",
    "error_code",
    "error_message",
}
TOP_LEVEL_KEYS = set(REQUIRED_KEYS) | OPTIONAL_KEYS
ALLOWED_TERMINAL_EVENTS = {"RUN_FINISHED", "RUN_ERROR"}
ALLOWED_MESSAGE_ROLES = {"assistant", "developer", "system", "tool", "user"}
MAX_REF_LENGTH = 96
MAX_MESSAGE_ID_LENGTH = 96
MAX_NAME_LENGTH = 60
MAX_CONTENT_LENGTH = 400
MAX_ERROR_CODE_LENGTH = 48
MAX_ERROR_MESSAGE_LENGTH = 200


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one AG-UI compacted message snapshot artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="AG-UI artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-ag-ui-<stem>.",
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

    parsed = _parse_rfc3339_datetime(value)
    return parsed.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def _validate_non_empty_string(value: Any, line_label: str, field_name: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{line_label}: {field_name} must be a non-empty string")
    return value.strip()


def _validate_opaque_ref(value: Any, line_label: str, field_name: str) -> str:
    ref = _validate_non_empty_string(value, line_label, field_name)
    if len(ref) > MAX_REF_LENGTH:
        raise ValueError(f"{line_label}: {field_name} must be at most {MAX_REF_LENGTH} characters")
    lowered = ref.lower()
    if lowered.startswith("http://") or lowered.startswith("https://") or "://" in lowered:
        raise ValueError(f"{line_label}: {field_name} must be an opaque id, not a URL")
    return ref


def _validate_message_id(value: Any, line_label: str) -> str:
    message_id = _validate_non_empty_string(value, line_label, "id")
    if len(message_id) > MAX_MESSAGE_ID_LENGTH:
        raise ValueError(f"{line_label}: id must be at most {MAX_MESSAGE_ID_LENGTH} characters")
    return message_id


def _validate_message_content(value: Any, line_label: str) -> str:
    content = _validate_non_empty_string(value, line_label, "content")
    if len(content) > MAX_CONTENT_LENGTH:
        raise ValueError(
            f"{line_label}: content must be a short string of at most {MAX_CONTENT_LENGTH} characters"
        )
    return content


def _validate_short_name(value: Any, line_label: str) -> str:
    name = _validate_non_empty_string(value, line_label, "name")
    if len(name) > MAX_NAME_LENGTH:
        raise ValueError(f"{line_label}: name must be at most {MAX_NAME_LENGTH} characters")
    return name


def _validate_messages(value: Any, line_label: str) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        raise ValueError(f"{line_label}: messages must be a list")
    if not value:
        raise ValueError(f"{line_label}: messages must be a non-empty list")

    normalized_messages: list[dict[str, Any]] = []
    seen_ids: set[str] = set()
    for index, message in enumerate(value):
        message_label = f"{line_label}: messages[{index}]"
        if not isinstance(message, dict):
            raise ValueError(f"{message_label}: message must be an object")

        missing = [key for key in ("id", "role", "content") if key not in message]
        if missing:
            raise ValueError(f"{message_label}: missing required keys: {', '.join(missing)}")

        unknown = set(message) - {"id", "role", "content", "name"}
        if unknown:
            raise ValueError(f"{message_label}: unsupported keys: {', '.join(sorted(unknown))}")

        message_id = _validate_message_id(message["id"], message_label)
        if message_id in seen_ids:
            raise ValueError(f"{message_label}: duplicate message id: {message_id}")
        seen_ids.add(message_id)

        role = _validate_non_empty_string(message["role"], message_label, "role")
        if role not in ALLOWED_MESSAGE_ROLES:
            allowed = ", ".join(sorted(ALLOWED_MESSAGE_ROLES))
            raise ValueError(f"{message_label}: role must be one of: {allowed}")

        normalized_message = {
            "id": message_id,
            "role": role,
            "content": _validate_message_content(message["content"], message_label),
        }
        if "name" in message:
            normalized_message["name"] = _validate_short_name(message["name"], message_label)
        normalized_messages.append(normalized_message)

    return normalized_messages


def _validate_error_code(value: Any, line_label: str) -> str:
    error_code = _validate_non_empty_string(value, line_label, "error_code")
    if len(error_code) > MAX_ERROR_CODE_LENGTH:
        raise ValueError(f"{line_label}: error_code must be at most {MAX_ERROR_CODE_LENGTH} characters")
    return error_code


def _validate_error_message(value: Any, line_label: str) -> str:
    error_message = _validate_non_empty_string(value, line_label, "error_message")
    if len(error_message) > MAX_ERROR_MESSAGE_LENGTH:
        raise ValueError(
            f"{line_label}: error_message must be at most {MAX_ERROR_MESSAGE_LENGTH} characters"
        )
    return error_message


def _normalized_record(record: dict[str, Any]) -> dict[str, Any]:
    missing = [key for key in REQUIRED_KEYS if key not in record]
    if missing:
        raise ValueError(f"artifact: missing required keys: {', '.join(missing)}")

    unknown = set(record) - TOP_LEVEL_KEYS
    if unknown:
        raise ValueError(f"artifact: unsupported top-level keys: {', '.join(sorted(unknown))}")

    if record["schema"] != EXTERNAL_SCHEMA:
        raise ValueError(f"artifact: expected schema {EXTERNAL_SCHEMA}, got {record['schema']}")
    if record["framework"] != "ag_ui":
        raise ValueError("artifact: framework must be ag_ui")
    if record["surface"] != "compacted_message_snapshot_artifact":
        raise ValueError("artifact: surface must be compacted_message_snapshot_artifact")

    started_at_dt = _parse_rfc3339_datetime(str(record["started_at"]))
    started_at = started_at_dt.isoformat().replace("+00:00", "Z")
    terminal_event = _validate_non_empty_string(record["terminal_event"], "artifact", "terminal_event")
    if terminal_event not in ALLOWED_TERMINAL_EVENTS:
        allowed = ", ".join(sorted(ALLOWED_TERMINAL_EVENTS))
        raise ValueError(f"artifact: terminal_event must be one of: {allowed}")

    normalized = {
        "schema": EXTERNAL_SCHEMA,
        "framework": "ag_ui",
        "surface": "compacted_message_snapshot_artifact",
        "thread_id_ref": _validate_opaque_ref(record["thread_id_ref"], "artifact", "thread_id_ref"),
        "run_id_ref": _validate_opaque_ref(record["run_id_ref"], "artifact", "run_id_ref"),
        "started_at": started_at,
        "messages": _validate_messages(record["messages"], "artifact"),
        "terminal_event": terminal_event,
    }

    if "finished_at" in record:
        finished_at_dt = _parse_rfc3339_datetime(str(record["finished_at"]))
        if finished_at_dt < started_at_dt:
            raise ValueError("artifact: finished_at must not be earlier than started_at")
        finished_at = finished_at_dt.isoformat().replace("+00:00", "Z")
        normalized["finished_at"] = finished_at
    if "parent_run_id_ref" in record:
        normalized["parent_run_id_ref"] = _validate_opaque_ref(
            record["parent_run_id_ref"], "artifact", "parent_run_id_ref"
        )

    if terminal_event == "RUN_ERROR":
        if "error_message" not in record:
            raise ValueError("artifact: RUN_ERROR artifacts must include error_message")
        normalized["error_message"] = _validate_error_message(record["error_message"], "artifact")
        if "error_code" in record:
            normalized["error_code"] = _validate_error_code(record["error_code"], "artifact")
    else:
        if "error_code" in record or "error_message" in record:
            raise ValueError("artifact: RUN_FINISHED artifacts must not include error_code or error_message")

    return normalized


def _build_event(normalized: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    data = {
        "external_system": "ag_ui",
        "external_surface": "compacted-message-snapshot-artifact",
        "external_schema": EXTERNAL_SCHEMA,
        "observed_upstream_time": normalized["started_at"],
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

    assay_run_id = args.assay_run_id or f"import-ag-ui-{args.input.stem}"
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
