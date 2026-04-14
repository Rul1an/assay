"""Map a frozen Mem0 Add Memories result artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any, Optional


PLACEHOLDER_EVENT_TYPE = "example.placeholder.mem0-add-memories-result"
PLACEHOLDER_SOURCE = "urn:example:assay:external:mem0:add-memories-result"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "mem0.add-memories.results.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "timestamp",
    "operation",
    "output_format",
    "results",
)
OPTIONAL_KEYS = {
    "user_ref",
    "agent_ref",
    "run_ref",
    "version_ref",
}
TOP_LEVEL_KEYS = set(REQUIRED_KEYS) | OPTIONAL_KEYS
ALLOWED_EVENTS = {"ADD", "UPDATE", "DELETE"}
MAX_REF_LENGTH = 80
MAX_MEMORY_LENGTH = 280


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one Mem0 Add Memories result artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="Mem0 artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-mem0-<stem>.",
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
    # This sample keeps the fixture corpus inside a small deterministic JSON
    # profile used by the interop examples. It is not a full RFC 8785 / JCS
    # implementation for arbitrary JSON inputs.
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
    return value.strip()


def _validate_short_ref(value: Any, line_label: str, field_name: str) -> str:
    ref = _validate_non_empty_string(value, line_label, field_name)
    if len(ref) > MAX_REF_LENGTH:
        raise ValueError(f"{line_label}: {field_name} must be at most {MAX_REF_LENGTH} characters")
    return ref


def _validate_memory(value: Any, line_label: str) -> str:
    memory = _validate_non_empty_string(value, line_label, "data.memory")
    if len(memory) > MAX_MEMORY_LENGTH:
        raise ValueError(
            f"{line_label}: data.memory must be a short string of at most {MAX_MEMORY_LENGTH} characters"
        )
    return memory


def _validate_result(value: Any, line_label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError(f"{line_label}: result must be an object")

    missing = [key for key in ("id", "event", "data") if key not in value]
    if missing:
        raise ValueError(f"{line_label}: missing required keys: {', '.join(missing)}")

    unknown = set(value) - {"id", "event", "data"}
    if unknown:
        raise ValueError(f"{line_label}: unsupported keys: {', '.join(sorted(unknown))}")

    event = _validate_non_empty_string(value["event"], line_label, "event")
    if event not in ALLOWED_EVENTS:
        allowed = ", ".join(sorted(ALLOWED_EVENTS))
        raise ValueError(f"{line_label}: event must be one of: {allowed}")

    data = value["data"]
    if not isinstance(data, dict):
        raise ValueError(f"{line_label}: data must be an object")
    data_unknown = set(data) - {"memory"}
    if data_unknown:
        raise ValueError(f"{line_label}: unsupported data keys: {', '.join(sorted(data_unknown))}")
    if "memory" not in data:
        raise ValueError(f"{line_label}: data must include memory")

    return {
        "id": _validate_short_ref(value["id"], line_label, "id"),
        "event": event,
        "data": {"memory": _validate_memory(data["memory"], line_label)},
    }


def _validate_results(value: Any, line_label: str) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        raise ValueError(f"{line_label}: results must be a list")
    if not value:
        raise ValueError(f"{line_label}: results must be a non-empty list")
    return [_validate_result(item, f"{line_label}: results[{index}]") for index, item in enumerate(value)]


def _normalized_record(record: dict[str, Any]) -> dict[str, Any]:
    missing = [key for key in REQUIRED_KEYS if key not in record]
    if missing:
        raise ValueError(f"artifact: missing required keys: {', '.join(missing)}")

    unknown = set(record) - TOP_LEVEL_KEYS
    if unknown:
        raise ValueError(f"artifact: unsupported top-level keys: {', '.join(sorted(unknown))}")

    if record["schema"] != EXTERNAL_SCHEMA:
        raise ValueError(f"artifact: expected schema {EXTERNAL_SCHEMA}, got {record['schema']}")
    if record["framework"] != "mem0":
        raise ValueError("artifact: framework must be mem0")
    if record["surface"] != "add_memories_results":
        raise ValueError("artifact: surface must be add_memories_results")
    if record["operation"] != "add_memories":
        raise ValueError("artifact: operation must be add_memories")
    if record["output_format"] != "v1.1":
        raise ValueError("artifact: output_format must be v1.1")

    normalized = {
        "schema": EXTERNAL_SCHEMA,
        "framework": "mem0",
        "surface": "add_memories_results",
        "timestamp": _parse_rfc3339_utc(str(record["timestamp"])),
        "operation": "add_memories",
        "output_format": "v1.1",
        "results": _validate_results(record["results"], "artifact"),
    }

    for field in ("user_ref", "agent_ref", "run_ref", "version_ref"):
        if field in record:
            normalized[field] = _validate_short_ref(record[field], "artifact", field)

    return normalized


def _build_event(normalized: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    data = {
        "external_system": "mem0",
        "external_surface": "add-memories-results",
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

    assay_run_id = args.assay_run_id or f"import-mem0-{args.input.stem}"
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
