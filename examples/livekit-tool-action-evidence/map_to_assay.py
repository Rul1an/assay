"""Map a frozen LiveKit function-tool event into Assay-shaped placeholders."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any


PLACEHOLDER_EVENT_TYPE = "example.placeholder.livekit-function-tool-call"
PLACEHOLDER_SOURCE = "urn:example:assay:external:livekit:function-tool-call"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "livekit.function-tools-executed.export.v1"
FUTURE_RECEIPT_SCHEMA = "assay.receipt.livekit.tool-action.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "runtime_mode",
    "event_ref",
    "created_at",
    "function_calls",
    "function_call_outputs",
)
OPTIONAL_TOP_LEVEL_KEYS = {
    "type",
    "has_tool_reply",
    "has_agent_handoff",
}
FORBIDDEN_TOP_LEVEL_KEY_MESSAGES = {
    "transcript": "artifact: transcript import is out of scope for LiveKit tool-action v1",
    "audio": "artifact: audio import is out of scope for LiveKit tool-action v1",
    "user_input": "artifact: raw user input is out of scope for LiveKit tool-action v1",
    "model_output": "artifact: raw model output is out of scope for LiveKit tool-action v1",
    "usage": "artifact: usage telemetry is out of scope for LiveKit tool-action v1",
    "latency": "artifact: latency telemetry is out of scope for LiveKit tool-action v1",
    "room_state": "artifact: room state is out of scope for LiveKit tool-action v1",
    "participant_identity": "artifact: participant identity is out of scope for LiveKit tool-action v1",
    "trace": "artifact: full trace payloads are out of scope for LiveKit tool-action v1",
    "spans": "artifact: full span payloads are out of scope for LiveKit tool-action v1",
}
TOP_LEVEL_KEYS = set(REQUIRED_KEYS) | OPTIONAL_TOP_LEVEL_KEYS | set(FORBIDDEN_TOP_LEVEL_KEY_MESSAGES)
CALL_KEYS = {"id", "type", "call_id", "name", "arguments", "arguments_ref", "created_at", "group_id"}
OUTPUT_KEYS = {"id", "type", "call_id", "name", "output", "output_ref", "is_error", "created_at"}
MAX_REF_LENGTH = 240
MAX_NAME_LENGTH = 160


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one LiveKit function_tools_executed artifact into Assay-shaped placeholder envelopes."
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
        help="Optional Assay run id override. Defaults to import-livekit-tool-action-<stem>.",
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
    return json.dumps(
        _normalize_for_hash(value),
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


def _parse_rfc3339_utc(value: str | None) -> str:
    if value is None:
        return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")

    normalized = f"{value[:-1]}+00:00" if value.endswith("Z") else value
    try:
        parsed = datetime.fromisoformat(normalized)
    except ValueError as exc:
        raise ValueError(f"invalid RFC3339 timestamp: {value}") from exc
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def _timestamp_to_rfc3339(value: Any, field_name: str) -> str:
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        if not math.isfinite(float(value)):
            raise ValueError(f"artifact: {field_name} must be finite")
        return datetime.fromtimestamp(float(value), timezone.utc).isoformat().replace("+00:00", "Z")
    if isinstance(value, str):
        return _parse_rfc3339_utc(value)
    raise ValueError(f"artifact: {field_name} must be a unix timestamp or RFC3339 string")


def _validate_non_empty_string(value: Any, field_name: str, max_length: int = MAX_REF_LENGTH) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"artifact: {field_name} must be a non-empty string")
    stripped = value.strip()
    if len(stripped) > max_length:
        raise ValueError(f"artifact: {field_name} must be at most {max_length} characters")
    if "\n" in stripped or "\r" in stripped:
        raise ValueError(f"artifact: {field_name} must be a single-line string")
    return stripped


def _validate_optional_bool(value: Any, field_name: str) -> bool:
    if not isinstance(value, bool):
        raise ValueError(f"artifact: {field_name} must be a boolean")
    return value


def _validate_call(value: Any, index: int) -> dict[str, Any]:
    label = f"function_calls[{index}]"
    if not isinstance(value, dict):
        raise ValueError(f"artifact: {label} must be an object")
    unknown = set(value) - CALL_KEYS
    if unknown:
        raise ValueError(f"artifact: unsupported {label} keys: {', '.join(sorted(unknown))}")
    if "name" not in value:
        raise ValueError(f"artifact: {label}.name is required")

    normalized: dict[str, Any] = {
        "name": _validate_non_empty_string(value["name"], f"{label}.name", MAX_NAME_LENGTH)
    }
    for key in ("id", "type", "call_id", "arguments_ref"):
        if key in value:
            normalized[key] = _validate_non_empty_string(value[key], f"{label}.{key}")
    if "group_id" in value:
        normalized["group_id"] = None if value["group_id"] is None else _validate_non_empty_string(
            value["group_id"], f"{label}.group_id"
        )
    if "created_at" in value:
        normalized["created_at"] = _timestamp_to_rfc3339(value["created_at"], f"{label}.created_at")
    if "arguments" in value:
        normalized["arguments"] = value["arguments"]
    return normalized


def _validate_output(value: Any, index: int) -> dict[str, Any] | None:
    label = f"function_call_outputs[{index}]"
    if value is None:
        return None
    if not isinstance(value, dict):
        raise ValueError(f"artifact: {label} must be an object")
    unknown = set(value) - OUTPUT_KEYS
    if unknown:
        raise ValueError(f"artifact: unsupported {label} keys: {', '.join(sorted(unknown))}")
    if "name" not in value:
        raise ValueError(f"artifact: {label}.name is required")

    normalized: dict[str, Any] = {
        "name": _validate_non_empty_string(value["name"], f"{label}.name", MAX_NAME_LENGTH)
    }
    for key in ("id", "type", "call_id", "output_ref"):
        if key in value:
            normalized[key] = _validate_non_empty_string(value[key], f"{label}.{key}")
    if "is_error" in value:
        normalized["is_error"] = _validate_optional_bool(value["is_error"], f"{label}.is_error")
    if "created_at" in value:
        normalized["created_at"] = _timestamp_to_rfc3339(value["created_at"], f"{label}.created_at")
    if "output" in value:
        normalized["output"] = value["output"]
    return normalized


def _raise_on_forbidden_top_level_keys(record: dict[str, Any]) -> None:
    present = [key for key in sorted(record) if key in FORBIDDEN_TOP_LEVEL_KEY_MESSAGES]
    if not present:
        return
    if len(present) == 1:
        raise ValueError(FORBIDDEN_TOP_LEVEL_KEY_MESSAGES[present[0]])
    details = "; ".join(FORBIDDEN_TOP_LEVEL_KEY_MESSAGES[key] for key in present)
    raise ValueError(f"artifact: multiple out-of-scope fields present: {details}")


def _normalize_record(record: dict[str, Any]) -> dict[str, Any]:
    _raise_on_forbidden_top_level_keys(record)
    unknown = set(record) - TOP_LEVEL_KEYS
    if unknown:
        raise ValueError(f"artifact: unsupported top-level keys: {', '.join(sorted(unknown))}")

    missing = [key for key in REQUIRED_KEYS if key not in record]
    if missing:
        raise ValueError(f"artifact: missing required keys: {', '.join(missing)}")
    if record.get("schema") != EXTERNAL_SCHEMA:
        raise ValueError(f"artifact: expected schema {EXTERNAL_SCHEMA}, got {record.get('schema')}")
    if record.get("framework") != "livekit_agents":
        raise ValueError("artifact: framework must be livekit_agents")
    if record.get("surface") != "function_tools_executed":
        raise ValueError("artifact: surface must be function_tools_executed")
    if record.get("runtime_mode") != "agent_session":
        raise ValueError("artifact: runtime_mode must be agent_session")
    if "type" in record and record["type"] != "function_tools_executed":
        raise ValueError("artifact: type must be function_tools_executed when present")

    calls_raw = record["function_calls"]
    outputs_raw = record["function_call_outputs"]
    if not isinstance(calls_raw, list) or not calls_raw:
        raise ValueError("artifact: function_calls must be a non-empty list")
    if not isinstance(outputs_raw, list):
        raise ValueError("artifact: function_call_outputs must be a list")
    if len(calls_raw) != len(outputs_raw):
        raise ValueError("artifact: function_calls and function_call_outputs must have matching counts")

    normalized = {
        "schema": EXTERNAL_SCHEMA,
        "framework": "livekit_agents",
        "surface": "function_tools_executed",
        "runtime_mode": "agent_session",
        "event_ref": _validate_non_empty_string(record["event_ref"], "event_ref"),
        "created_at": _timestamp_to_rfc3339(record["created_at"], "created_at"),
        "function_calls": [_validate_call(call, index) for index, call in enumerate(calls_raw)],
        "function_call_outputs": [_validate_output(output, index) for index, output in enumerate(outputs_raw)],
    }

    if "type" in record:
        normalized["type"] = "function_tools_executed"
    for key in ("has_tool_reply", "has_agent_handoff"):
        if key in record:
            normalized[key] = _validate_optional_bool(record[key], key)
    return normalized


def _pair_calls_outputs(normalized: dict[str, Any]) -> list[tuple[dict[str, Any], dict[str, Any] | None]]:
    calls = normalized["function_calls"]
    outputs = normalized["function_call_outputs"]
    paired = list(zip(calls, outputs))
    if all(
        "call_id" in call and output is not None and "call_id" in output
        for call, output in paired
    ):
        for index, (call, output) in enumerate(paired):
            if call["call_id"] != output["call_id"]:
                raise ValueError(
                    "artifact: call_id mismatch at index "
                    f"{index}: call has {call['call_id']!r}, output has {output['call_id']!r}"
                )
    return paired


def _hash_or_ref(record: dict[str, Any], raw_key: str, ref_key: str) -> tuple[str | None, str | None]:
    if raw_key in record and ref_key in record:
        raise ValueError(f"artifact: {raw_key} and {ref_key} must not both be present")
    if raw_key in record:
        return _sha256(record[raw_key]), None
    if ref_key in record:
        return None, record[ref_key]
    return None, None


def _build_receipt(
    normalized: dict[str, Any],
    call: dict[str, Any],
    output: dict[str, Any] | None,
    call_index: int,
    source_artifact_ref: str,
    source_artifact_digest: str,
    import_time: str,
) -> dict[str, Any]:
    function: dict[str, Any] = {
        "event_ref": normalized["event_ref"],
        "call_index": call_index,
        "name": call["name"],
    }
    for key in ("call_id", "group_id", "created_at"):
        if key in call and call[key] is not None:
            function[key] = call[key]
    arguments_hash, arguments_ref = _hash_or_ref(call, "arguments", "arguments_ref")
    if arguments_hash is not None:
        function["arguments_hash"] = arguments_hash
    if arguments_ref is not None:
        function["arguments_ref"] = arguments_ref

    if output is None:
        outcome: dict[str, Any] = {"completed": False}
    else:
        if call["name"] != output["name"]:
            raise ValueError(f"artifact: paired call/output names differ at index {call_index}")
        outcome = {
            "completed": True,
            "is_error": output.get("is_error", False),
        }
        output_hash, output_ref = _hash_or_ref(output, "output", "output_ref")
        if output_hash is not None:
            outcome["output_hash"] = output_hash
        if output_ref is not None:
            outcome["output_ref"] = output_ref
        if "created_at" in output:
            outcome["received_at"] = output["created_at"]

    event_context: dict[str, Any] = {
        "event_created_at": normalized["created_at"],
    }
    for key in ("has_tool_reply", "has_agent_handoff"):
        if key in normalized:
            event_context[key] = normalized[key]

    receipt: dict[str, Any] = {
        "schema": FUTURE_RECEIPT_SCHEMA,
        "source_system": "livekit_agents",
        "source_surface": "function_tools_executed",
        "source_artifact_ref": source_artifact_ref,
        "source_artifact_digest": source_artifact_digest,
        "reducer_version": "assay-livekit-function-tools-executed@0.1.0-placeholder",
        "imported_at": import_time,
        "function": function,
        "outcome": outcome,
        "event_context": event_context,
    }
    return receipt


def _build_event(
    receipt: dict[str, Any],
    assay_run_id: str,
    call_index: int,
    import_time: str,
) -> dict[str, Any]:
    data = {
        "external_system": "livekit_agents",
        "external_surface": "function-tools-executed",
        "external_schema": EXTERNAL_SCHEMA,
        "observed_upstream_time": receipt["event_context"]["event_created_at"],
        "receipt": receipt,
    }
    event = {
        "specversion": "1.0",
        "type": PLACEHOLDER_EVENT_TYPE,
        "source": PLACEHOLDER_SOURCE,
        "id": f"{assay_run_id}:{call_index}",
        "time": import_time,
        "datacontenttype": "application/json",
        "assayrunid": assay_run_id,
        "assayseq": call_index,
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
        normalized = _normalize_record(record)
        paired = _pair_calls_outputs(normalized)
        import_time = _parse_rfc3339_utc(args.import_time)
        source_artifact_ref = args.input.as_posix()
        source_artifact_digest = _sha256(normalized)
        events = [
            _build_event(
                _build_receipt(
                    normalized,
                    call,
                    output,
                    call_index,
                    source_artifact_ref,
                    source_artifact_digest,
                    import_time,
                ),
                args.assay_run_id or f"import-livekit-tool-action-{args.input.stem}",
                call_index,
                import_time,
            )
            for call_index, (call, output) in enumerate(paired)
        ]
    except (TypeError, ValueError) as exc:
        raise SystemExit(str(exc)) from exc

    args.output.parent.mkdir(parents=True, exist_ok=True)
    try:
        args.output.write_text("".join(_canonical_json(event) + "\n" for event in events), encoding="utf-8")
    except OSError as exc:
        raise SystemExit(str(exc)) from exc
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
