"""Map a frozen Stagehand observe-derived selector-scoped extract artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any, Optional


PLACEHOLDER_EVENT_TYPE = "example.placeholder.stagehand-selector-scoped-extract"
PLACEHOLDER_SOURCE = "urn:example:assay:external:stagehand:selector-scoped-extract"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "stagehand.selector-scoped-extract.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "timestamp",
    "observe_instruction",
    "extract_instruction",
    "selector_ref",
    "selector_source",
    "selector_kind",
    "result",
)
OPTIONAL_KEYS = {
    "scope_hint",
    "result_schema_ref",
    "cache_status",
    "page_ref",
    "run_ref",
    "metadata_ref",
}
TOP_LEVEL_KEYS = set(REQUIRED_KEYS) | OPTIONAL_KEYS
ALLOWED_SELECTOR_SOURCES = {"observe"}
ALLOWED_SELECTOR_KINDS = {"xpath", "css", "other"}
ALLOWED_CACHE_STATUS = {"HIT", "MISS"}
MAX_TEXT_LENGTH = 180
MAX_SELECTOR_LENGTH = 240
MAX_REF_LENGTH = 96
MAX_RESULT_KEYS = 8
MAX_RESULT_KEY_LENGTH = 40
MAX_RESULT_STRING_LENGTH = 120


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one Stagehand selector-scoped extract artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="Stagehand artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-stagehand-<stem>.",
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


def _validate_classifier(value: Any, line_label: str, field_name: str) -> str:
    classifier = _validate_non_empty_string(value, line_label, field_name, MAX_RESULT_KEY_LENGTH)
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


def _validate_selector_ref(value: Any) -> str:
    selector = _validate_non_empty_string(value, "artifact", "selector_ref", MAX_SELECTOR_LENGTH)
    return selector


def _validate_timestamp(value: Any) -> str:
    return _parse_rfc3339_datetime(
        _validate_non_empty_string(value, "artifact", "timestamp", MAX_TEXT_LENGTH)
    ).isoformat().replace("+00:00", "Z")


def _validate_instruction(value: Any, field_name: str) -> str:
    return _validate_non_empty_string(value, "artifact", field_name, MAX_TEXT_LENGTH)


def _validate_result_value(value: Any, line_label: str, field_name: str) -> Any:
    if isinstance(value, bool):
        return value
    if isinstance(value, int):
        return value
    if isinstance(value, float):
        if not math.isfinite(value):
            raise ValueError(f"{line_label}: {field_name} must be a finite number")
        if not value.is_integer():
            raise ValueError(f"{line_label}: {field_name} must be an integer-valued number in this sample")
        return int(value)
    if isinstance(value, str):
        normalized = value.strip()
        if not normalized:
            raise ValueError(f"{line_label}: {field_name} must not be an empty string")
        if len(normalized) > MAX_RESULT_STRING_LENGTH:
            raise ValueError(
                f"{line_label}: {field_name} must be at most {MAX_RESULT_STRING_LENGTH} characters"
            )
        return normalized
    raise ValueError(f"{line_label}: {field_name} must be a bounded scalar value")


def _validate_result(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError("artifact: result must be an object")
    if len(value) > MAX_RESULT_KEYS:
        raise ValueError(f"artifact: result must contain at most {MAX_RESULT_KEYS} fields")

    normalized: dict[str, Any] = {}
    for key, nested in value.items():
        if not isinstance(key, str) or not key:
            raise ValueError("artifact: result keys must be non-empty strings")
        classifier = _validate_classifier(key, "artifact: result", "field")
        if classifier in normalized:
            raise ValueError(f"artifact: duplicate normalized result key: {classifier}")
        normalized[classifier] = _validate_result_value(nested, "artifact: result", classifier)
    return normalized


def _normalized_record(record: dict[str, Any]) -> dict[str, Any]:
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
    if framework != "stagehand":
        raise ValueError("artifact: framework must be stagehand")

    surface = _validate_classifier(record["surface"], "artifact", "surface")
    if surface != "observe_derived_selector_scoped_extract":
        raise ValueError("artifact: surface must be observe_derived_selector_scoped_extract")

    selector_source = _validate_classifier(record["selector_source"], "artifact", "selector_source")
    if selector_source not in ALLOWED_SELECTOR_SOURCES:
        allowed = ", ".join(sorted(ALLOWED_SELECTOR_SOURCES))
        raise ValueError(f"artifact: selector_source must be one of: {allowed}")

    selector_kind = _validate_classifier(record["selector_kind"], "artifact", "selector_kind")
    if selector_kind not in ALLOWED_SELECTOR_KINDS:
        allowed = ", ".join(sorted(ALLOWED_SELECTOR_KINDS))
        raise ValueError(f"artifact: selector_kind must be one of: {allowed}")

    normalized = {
        "schema": schema,
        "framework": framework,
        "surface": surface,
        "timestamp": _validate_timestamp(record["timestamp"]),
        "observe_instruction": _validate_instruction(record["observe_instruction"], "observe_instruction"),
        "extract_instruction": _validate_instruction(record["extract_instruction"], "extract_instruction"),
        "selector_ref": _validate_selector_ref(record["selector_ref"]),
        "selector_source": selector_source,
        "selector_kind": selector_kind,
        "result": _validate_result(record["result"]),
    }

    if "scope_hint" in record:
        normalized["scope_hint"] = _validate_classifier(record["scope_hint"], "artifact", "scope_hint")

    if "result_schema_ref" in record:
        normalized["result_schema_ref"] = _validate_opaque_ref(
            record["result_schema_ref"], "artifact", "result_schema_ref"
        )

    if "cache_status" in record:
        cache_status = _validate_non_empty_string(record["cache_status"], "artifact", "cache_status", 8)
        if cache_status not in ALLOWED_CACHE_STATUS:
            allowed = ", ".join(sorted(ALLOWED_CACHE_STATUS))
            raise ValueError(f"artifact: cache_status must be one of: {allowed}")
        normalized["cache_status"] = cache_status

    if "page_ref" in record:
        normalized["page_ref"] = _validate_opaque_ref(record["page_ref"], "artifact", "page_ref")

    if "run_ref" in record:
        normalized["run_ref"] = _validate_opaque_ref(record["run_ref"], "artifact", "run_ref")

    if "metadata_ref" in record:
        normalized["metadata_ref"] = _validate_opaque_ref(record["metadata_ref"], "artifact", "metadata_ref")

    return normalized


def _build_event(normalized: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    data = {
        "external_system": "stagehand",
        "external_surface": "observe-derived-selector-scoped-extract",
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

    assay_run_id = args.assay_run_id or f"import-stagehand-{args.input.stem}"
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
