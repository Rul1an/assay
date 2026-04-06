"""Map a frozen LangGraph tasks-v2 NDJSON export into Assay-shaped placeholder envelopes."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any


PLACEHOLDER_EVENT_TYPE = "example.placeholder.langgraph-task-event"
PLACEHOLDER_SOURCE = "urn:example:assay:external:langgraph:tasks-v2"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "langgraph.stream.tasks.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "record_type",
    "event_phase",
    "stream_mode",
    "stream_version",
    "thread_id",
    "task_ref",
    "task_name",
    "timestamp",
)


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map LangGraph tasks-v2 NDJSON into Assay-shaped placeholder envelopes."
    )
    parser.add_argument("input", type=Path, help="LangGraph NDJSON export to read.")
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
        help="Optional Assay run id override. Defaults to import-langgraph-<stem>.",
    )
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="Allow overwriting the output file if it already exists.",
    )
    return parser.parse_args()


def _normalize_for_hash(value: Any) -> Any:
    if value is None or isinstance(value, (str, int, bool)):
        return value
    if isinstance(value, float):
        if not math.isfinite(value):
            raise ValueError("non-finite floats are not valid in canonical JSON")
        if value.is_integer():
            return int(value)
        return value
    if isinstance(value, dict):
        return {str(key): _normalize_for_hash(nested) for key, nested in value.items()}
    if isinstance(value, list):
        return [_normalize_for_hash(item) for item in value]
    if isinstance(value, tuple):
        return [_normalize_for_hash(item) for item in value]
    raise TypeError(f"unsupported canonical JSON value: {type(value).__name__}")


def _canonical_json(value: Any) -> str:
    # This sample keeps the fixture corpus in the JCS-safe subset
    # (objects, arrays, strings, bools, null, and integer-valued numbers),
    # so deterministic sorted-key JSON matches the bytes Assay hashes today.
    # It is not a full RFC 8785 implementation for arbitrary JSON inputs.
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


def _parse_rfc3339_utc(value: str | None) -> str:
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


def _validate_record(record: dict[str, Any], line_number: int) -> None:
    missing = [key for key in REQUIRED_KEYS if key not in record]
    if missing:
        joined = ", ".join(missing)
        raise ValueError(f"line {line_number}: missing required keys: {joined}")
    if record["schema"] != EXTERNAL_SCHEMA:
        raise ValueError(
            f"line {line_number}: expected schema {EXTERNAL_SCHEMA}, got {record['schema']}"
        )
    if record["framework"] != "langgraph":
        raise ValueError(f"line {line_number}: framework must be langgraph")
    if record["stream_mode"] != "tasks":
        raise ValueError(f"line {line_number}: stream_mode must be tasks")
    if record["stream_version"] != "v2":
        raise ValueError(f"line {line_number}: stream_version must be v2")

    record_type = record["record_type"]
    if record_type == "task_start" and "task_input_hash" not in record:
        raise ValueError(f"line {line_number}: task_start missing task_input_hash")
    if record_type == "task_result" and "task_result_hash" not in record:
        raise ValueError(f"line {line_number}: task_result missing task_result_hash")
    if record_type == "stream_error" and "error" not in record:
        raise ValueError(f"line {line_number}: stream_error missing error")


def _normalized_record(record: dict[str, Any], line_number: int) -> dict[str, Any]:
    normalized = dict(record)
    try:
        normalized["timestamp"] = _parse_rfc3339_utc(str(record["timestamp"]))
    except ValueError as exc:
        raise ValueError(f"line {line_number}: {exc}") from exc
    return normalized


def _build_event(
    record: dict[str, Any],
    line_number: int,
    assay_run_id: str,
    assay_seq: int,
    import_time: str,
) -> dict[str, Any]:
    normalized = _normalized_record(record, line_number)
    data = {
        "external_system": "langgraph",
        "external_surface": "stream/tasks/v2",
        "external_schema": EXTERNAL_SCHEMA,
        "observed_upstream_time": normalized["timestamp"],
        "observed": record,
    }
    event = {
        "specversion": "1.0",
        "type": PLACEHOLDER_EVENT_TYPE,
        "source": PLACEHOLDER_SOURCE,
        "id": f"{assay_run_id}:{assay_seq}",
        "time": import_time,
        "datacontenttype": "application/json",
        "assayrunid": assay_run_id,
        "assayseq": assay_seq,
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
        import_time = _parse_rfc3339_utc(args.import_time)
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc
    assay_run_id = args.assay_run_id or f"import-langgraph-{args.input.stem}"

    mapped: list[dict[str, Any]] = []
    with args.input.open("r", encoding="utf-8") as handle:
        for line_number, raw_line in enumerate(handle, start=1):
            line = raw_line.strip()
            if not line:
                continue
            try:
                record = json.loads(line)
            except json.JSONDecodeError as exc:
                raise SystemExit(f"line {line_number}: invalid JSON: {exc.msg}") from exc
            if not isinstance(record, dict):
                raise SystemExit(f"line {line_number}: expected a JSON object")
            try:
                _validate_record(record, line_number)
            except ValueError as exc:
                raise SystemExit(str(exc)) from exc
            try:
                mapped.append(
                    _build_event(record, line_number, assay_run_id, len(mapped), import_time)
                )
            except ValueError as exc:
                raise SystemExit(str(exc)) from exc

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", encoding="utf-8") as handle:
        for event in mapped:
            handle.write(_canonical_json(event))
            handle.write("\n")

    print(f"Wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
