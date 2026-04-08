"""Map a frozen Langfuse experiment-result artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any


PLACEHOLDER_EVENT_TYPE = "example.placeholder.langfuse-experiment-result"
PLACEHOLDER_SOURCE = "urn:example:assay:external:langfuse:experiment-item-result"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "langfuse.experiment-item-result.export.v1"
REQUIRED_KEYS = (
    "schema",
    "platform",
    "surface",
    "experiment_name",
    "dataset_name",
    "dataset_version_ref",
    "item_ref",
    "timestamp",
    "output_ref",
    "evaluations",
)
OPTIONAL_KEYS = {
    "run_ref",
    "trace_ref",
    "experiment_description_ref",
    "metadata_ref",
    "aggregate_scores",
}
ALLOWED_DATA_TYPES = {"numeric", "boolean", "categorical"}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one Langfuse experiment-result artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="Langfuse experiment-result artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-langfuse-<stem>.",
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


def _validate_non_negative_int(value: Any, line_label: str, field_name: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool):
        raise ValueError(f"{line_label}: {field_name} must be an integer")
    if value < 0:
        raise ValueError(f"{line_label}: {field_name} must be >= 0")
    return value


def _validate_optional_ref(value: Any, line_label: str, field_name: str) -> None:
    _validate_non_empty_string(value, line_label, field_name)


def _validate_evaluation_value(value: Any, data_type: str, line_label: str) -> None:
    if data_type == "numeric":
        points = _validate_non_negative_int(value, line_label, "value")
        if points > 100:
            raise ValueError(f"{line_label}: value must be <= 100 when data_type is numeric")
        return
    if data_type == "boolean":
        if not isinstance(value, bool):
            raise ValueError(f"{line_label}: value must be a boolean when data_type is boolean")
        return
    if data_type == "categorical":
        _validate_non_empty_string(value, line_label, "value")
        return
    raise AssertionError(f"unhandled data_type: {data_type}")


def _validate_evaluations(value: Any, line_label: str) -> None:
    if not isinstance(value, list) or not value:
        raise ValueError(f"{line_label}: evaluations must be a non-empty list")

    seen_names: set[str] = set()
    for index, evaluation in enumerate(value):
        item_label = f"{line_label}: evaluations[{index}]"
        if not isinstance(evaluation, dict):
            raise ValueError(f"{item_label} must be a JSON object")

        required = {"name", "data_type", "value"}
        missing = [key for key in required if key not in evaluation]
        if missing:
            joined = ", ".join(sorted(missing))
            raise ValueError(f"{item_label} missing required keys: {joined}")

        unknown = set(evaluation) - required
        if unknown:
            joined = ", ".join(sorted(unknown))
            raise ValueError(f"{item_label} contains unsupported keys: {joined}")

        name = _validate_non_empty_string(evaluation["name"], item_label, "name")
        if name in seen_names:
            raise ValueError(f"{item_label}: duplicate evaluation name: {name}")
        seen_names.add(name)

        data_type = _validate_non_empty_string(evaluation["data_type"], item_label, "data_type")
        if data_type not in ALLOWED_DATA_TYPES:
            allowed = ", ".join(sorted(ALLOWED_DATA_TYPES))
            raise ValueError(f"{item_label}: data_type must be one of: {allowed}")

        _validate_evaluation_value(evaluation["value"], data_type, item_label)


def _validate_aggregate_scores(value: Any, line_label: str) -> None:
    if not isinstance(value, dict) or not value:
        raise ValueError(f"{line_label}: aggregate_scores must be a non-empty JSON object")
    for key, nested in value.items():
        if not isinstance(key, str) or not key.strip():
            raise ValueError(f"{line_label}: aggregate_scores keys must be non-empty strings")
        if isinstance(nested, bool):
            continue
        if isinstance(nested, int):
            if nested < 0 or nested > 100:
                raise ValueError(
                    f"{line_label}: aggregate_scores.{key} must be between 0 and 100"
                )
            continue
        if isinstance(nested, str) and nested.strip():
            continue
        raise ValueError(
            f"{line_label}: aggregate_scores.{key} must be a non-empty string, boolean, or bounded integer"
        )


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
    if record["platform"] != "langfuse":
        raise ValueError(f"{line_label}: platform must be langfuse")
    if record["surface"] != "experiment_item_result":
        raise ValueError(f"{line_label}: surface must be experiment_item_result")

    for field in (
        "experiment_name",
        "dataset_name",
        "dataset_version_ref",
        "item_ref",
        "output_ref",
    ):
        _validate_non_empty_string(record[field], line_label, field)

    _validate_evaluations(record["evaluations"], line_label)

    for field in ("run_ref", "trace_ref", "experiment_description_ref", "metadata_ref"):
        if field in record:
            _validate_optional_ref(record[field], line_label, field)
    if "aggregate_scores" in record:
        _validate_aggregate_scores(record["aggregate_scores"], line_label)


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
        "external_system": "langfuse",
        "external_surface": "experiment-item-result",
        "external_schema": EXTERNAL_SCHEMA,
        "observed_upstream_time": normalized["timestamp"],
        "observed": record,
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
    except (json.JSONDecodeError, ValueError) as exc:
        raise SystemExit(str(exc)) from exc

    if not isinstance(record, dict):
        raise SystemExit("artifact: top-level JSON value must be an object")

    try:
        _validate_record(record, "artifact")
        assay_run_id = args.assay_run_id or f"import-langfuse-{args.input.stem}"
        import_time = _parse_rfc3339_utc(args.import_time)
        event = _build_event(record, assay_run_id, import_time)
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", encoding="utf-8") as handle:
        handle.write(_canonical_json(event))
        handle.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
