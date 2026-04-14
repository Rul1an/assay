"""Map a frozen Mastra score-event artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any, Optional


PLACEHOLDER_EVENT_TYPE = "example.placeholder.mastra-score-event"
PLACEHOLDER_SOURCE = "urn:example:assay:external:mastra:score-event"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "mastra.score-event.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "timestamp",
    "score",
    "target_ref",
)
OPTIONAL_KEYS = {
    "scorer_id",
    "scorer_name",
    "target_entity_type",
    "reason",
    "trace_id_ref",
    "span_id_ref",
    "scorer_version",
    "score_source",
    "metadata_ref",
}
TOP_LEVEL_KEYS = set(REQUIRED_KEYS) | OPTIONAL_KEYS
ALLOWED_SCORE_SOURCES = {"live", "trace", "experiment"}
MAX_REASON_LENGTH = 200
MAX_REF_LENGTH = 120
MAX_CLASSIFIER_LENGTH = 48
MAX_NAME_LENGTH = 120


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one Mastra score-event artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="Mastra artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-mastra-score-<stem>.",
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
        return value
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


def _validate_short_string(
    value: Any, line_label: str, field_name: str, max_length: int = MAX_REF_LENGTH
) -> str:
    text = _validate_non_empty_string(value, line_label, field_name)
    if len(text) > max_length:
        raise ValueError(f"{line_label}: {field_name} must be at most {max_length} characters")
    return text


def _validate_opaque_ref(value: Any, line_label: str, field_name: str) -> str:
    ref = _validate_short_string(value, line_label, field_name)
    allowed = set("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789:_-.")
    if "://" in ref or any(char not in allowed for char in ref):
        raise ValueError(
            f"{line_label}: {field_name} must be an opaque id using letters, digits, ':', '_', '-' or '.'"
        )
    return ref


def _validate_classifier(value: Any, line_label: str, field_name: str) -> str:
    classifier = _validate_short_string(value, line_label, field_name, MAX_CLASSIFIER_LENGTH)
    allowed = set("abcdefghijklmnopqrstuvwxyz0123456789_-")
    if any(char not in allowed for char in classifier):
        raise ValueError(
            f"{line_label}: {field_name} must use lowercase letters, digits, '_' or '-'"
        )
    return classifier


def _validate_reason(value: Any, line_label: str) -> str:
    reason = _validate_non_empty_string(value, line_label, "reason")
    if len(reason) > MAX_REASON_LENGTH:
        raise ValueError(
            f"{line_label}: reason must be a short string of at most {MAX_REASON_LENGTH} characters"
        )
    if "\n" in reason or "\r" in reason:
        raise ValueError("artifact: reason must stay single-line")
    return reason


def _validate_score(value: Any, line_label: str) -> float | int:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"{line_label}: score must be a number")
    if not math.isfinite(float(value)):
        raise ValueError(f"{line_label}: score must be finite")
    return value


def _normalized_record(record: dict[str, Any]) -> dict[str, Any]:
    missing = [key for key in REQUIRED_KEYS if key not in record]
    if missing:
        raise ValueError(f"artifact: missing required keys: {', '.join(missing)}")

    unknown = set(record) - TOP_LEVEL_KEYS
    if unknown:
        raise ValueError(f"artifact: unsupported top-level keys: {', '.join(sorted(unknown))}")

    if record["schema"] != EXTERNAL_SCHEMA:
        raise ValueError(f"artifact: expected schema {EXTERNAL_SCHEMA}, got {record['schema']}")
    if record["framework"] != "mastra":
        raise ValueError("artifact: framework must be mastra")
    if record["surface"] != "observability_score_event":
        raise ValueError("artifact: surface must be observability_score_event")

    normalized = {
        "schema": EXTERNAL_SCHEMA,
        "framework": "mastra",
        "surface": "observability_score_event",
        "timestamp": _parse_rfc3339_utc(str(record["timestamp"])),
        "score": float(_validate_score(record["score"], "artifact")),
        "target_ref": _validate_opaque_ref(record["target_ref"], "artifact", "target_ref"),
    }

    scorer_id = record.get("scorer_id")
    scorer_name = record.get("scorer_name")
    if scorer_id is None and scorer_name is None:
        raise ValueError("artifact: at least one scorer identity field is required: scorer_id or scorer_name")

    if scorer_id is not None:
        normalized["scorer_id"] = _validate_short_string(scorer_id, "artifact", "scorer_id")
    if scorer_name is not None:
        normalized["scorer_name"] = _validate_short_string(
            scorer_name, "artifact", "scorer_name", MAX_NAME_LENGTH
        )
    if "target_entity_type" in record:
        normalized["target_entity_type"] = _validate_classifier(
            record["target_entity_type"], "artifact", "target_entity_type"
        )

    if "reason" in record:
        normalized["reason"] = _validate_reason(record["reason"], "artifact")
    if "trace_id_ref" in record:
        normalized["trace_id_ref"] = _validate_opaque_ref(record["trace_id_ref"], "artifact", "trace_id_ref")
    if "span_id_ref" in record:
        normalized["span_id_ref"] = _validate_opaque_ref(record["span_id_ref"], "artifact", "span_id_ref")
    if "scorer_version" in record:
        normalized["scorer_version"] = _validate_short_string(
            record["scorer_version"], "artifact", "scorer_version", 64
        )
    if "score_source" in record:
        score_source = _validate_classifier(record["score_source"], "artifact", "score_source")
        if score_source not in ALLOWED_SCORE_SOURCES:
            allowed = ", ".join(sorted(ALLOWED_SCORE_SOURCES))
            raise ValueError(f"artifact: score_source must be one of: {allowed}")
        normalized["score_source"] = score_source
    if "metadata_ref" in record:
        normalized["metadata_ref"] = _validate_opaque_ref(record["metadata_ref"], "artifact", "metadata_ref")

    return normalized


def _build_event(normalized: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    data = {
        "external_system": "mastra",
        "external_surface": "observability-score-event",
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

    assay_run_id = args.assay_run_id or f"import-mastra-score-{args.input.stem}"
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
