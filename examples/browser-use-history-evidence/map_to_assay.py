"""Map a frozen Browser Use history artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
from typing import Any


PLACEHOLDER_EVENT_TYPE = "example.placeholder.browser-use-history"
PLACEHOLDER_SOURCE = "urn:example:assay:external:browser-use:history"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "browser-use.agent-history.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "task_label",
    "timestamp",
    "outcome",
    "action_history",
    "final_result",
    "errors",
)
OPTIONAL_KEYS = {
    "structured_output_ref",
    "history_summary",
    "browser_ref",
    "url_ref",
}
ALLOWED_OUTCOMES = {"succeeded", "failed"}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one Browser Use history artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="Browser Use history artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-browser-use-<stem>.",
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
    if isinstance(value, dict):
        return {str(key): _normalize_for_hash(nested) for key, nested in value.items()}
    if isinstance(value, list):
        return [_normalize_for_hash(item) for item in value]
    if isinstance(value, tuple):
        return [_normalize_for_hash(item) for item in value]
    raise TypeError(f"unsupported canonical JSON value: {type(value).__name__}")


def _canonical_json(value: Any) -> str:
    # This sample emits deterministic sorted-key JSON for the validated fixture
    # corpus, but it is not a full RFC 8785 / JCS implementation for arbitrary
    # JSON inputs or cross-implementation hashing.
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


def _validate_non_empty_string(value: Any, line_label: str, field_name: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{line_label}: {field_name} must be a non-empty string")
    return value


def _validate_optional_ref(value: Any, line_label: str, field_name: str) -> None:
    _validate_non_empty_string(value, line_label, field_name)


def _validate_action_history(value: Any, line_label: str) -> list[str]:
    if not isinstance(value, list) or not value:
        raise ValueError(f"{line_label}: action_history must be a non-empty list")
    normalized: list[str] = []
    for index, item in enumerate(value):
        action = _validate_non_empty_string(item, line_label, f"action_history[{index}]")
        normalized.append(action)
    return normalized


def _validate_errors(value: Any, line_label: str) -> list[str]:
    if not isinstance(value, list):
        raise ValueError(f"{line_label}: errors must be a list")
    normalized: list[str] = []
    for index, item in enumerate(value):
        error = _validate_non_empty_string(item, line_label, f"errors[{index}]")
        normalized.append(error)
    return normalized


def _validate_record(record: dict[str, Any], line_label: str) -> None:
    missing = [key for key in REQUIRED_KEYS if key not in record]
    if missing:
        joined = ", ".join(missing)
        raise ValueError(f"{line_label}: missing required keys: {joined}")

    unknown = set(record) - set(REQUIRED_KEYS) - OPTIONAL_KEYS
    if unknown:
        joined = ", ".join(sorted(unknown))
        raise ValueError(f"{line_label}: unsupported top-level keys: {joined}")

    if record["schema"] != EXTERNAL_SCHEMA:
        raise ValueError(
            f"{line_label}: expected schema {EXTERNAL_SCHEMA}, got {record['schema']}"
        )
    if record["framework"] != "browser-use":
        raise ValueError(f"{line_label}: framework must be browser-use")
    if record["surface"] != "agent_history_list":
        raise ValueError(f"{line_label}: surface must be agent_history_list")

    _validate_non_empty_string(record["task_label"], line_label, "task_label")
    _validate_non_empty_string(record["final_result"], line_label, "final_result")

    if not isinstance(record["outcome"], str) or record["outcome"] not in ALLOWED_OUTCOMES:
        allowed = ", ".join(sorted(ALLOWED_OUTCOMES))
        raise ValueError(f"{line_label}: outcome must be one of: {allowed}")

    action_history = _validate_action_history(record["action_history"], line_label)
    errors = _validate_errors(record["errors"], line_label)

    if record["outcome"] == "succeeded" and errors:
        raise ValueError(f"{line_label}: succeeded artifacts must not carry errors")
    if record["outcome"] == "failed" and not errors:
        raise ValueError(f"{line_label}: failed artifacts must carry at least one error")

    if "history_summary" in record:
        _validate_non_empty_string(record["history_summary"], line_label, "history_summary")
    for field in ("structured_output_ref", "browser_ref", "url_ref"):
        if field in record:
            _validate_optional_ref(record[field], line_label, field)

    # Keep one simple consistency rule for the frozen sample shape.
    if record["outcome"] == "succeeded" and len(action_history) < 2:
        raise ValueError(
            f"{line_label}: succeeded artifacts must carry at least two action_history entries"
        )


def _normalized_record(record: dict[str, Any], line_label: str) -> dict[str, Any]:
    normalized = dict(record)
    try:
        normalized["timestamp"] = _parse_rfc3339_utc(str(record["timestamp"]))
    except ValueError as exc:
        raise ValueError(f"{line_label}: {exc}") from exc
    return normalized


def _build_event(record: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    normalized = _normalized_record(record, "artifact")
    data = {
        "external_system": "browser-use",
        "external_surface": "agent-history-list",
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

    with args.input.open("r", encoding="utf-8") as handle:
        record = json.load(handle)

    if not isinstance(record, dict):
        raise SystemExit("artifact: top-level JSON value must be an object")

    _validate_record(record, "artifact")

    assay_run_id = args.assay_run_id or f"import-browser-use-{args.input.stem}"
    import_time = _parse_rfc3339_utc(args.import_time)
    event = _build_event(record, assay_run_id, import_time)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", encoding="utf-8") as handle:
        handle.write(json.dumps(event, separators=(",", ":"), ensure_ascii=False))
        handle.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
