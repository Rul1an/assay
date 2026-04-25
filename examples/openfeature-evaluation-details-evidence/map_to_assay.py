"""Map a frozen OpenFeature EvaluationDetails artifact into an Assay-shaped envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any


PLACEHOLDER_EVENT_TYPE = "example.placeholder.openfeature-evaluation-details"
PLACEHOLDER_SOURCE = "urn:example:assay:external:openfeature:evaluation-details"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "openfeature.evaluation-details.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "target_kind",
    "flag_key",
    "result",
)
OPTIONAL_TOP_LEVEL_KEYS = {"flag_metadata_ref"}
FORBIDDEN_TOP_LEVEL_KEY_MESSAGES = {
    "value": "artifact: reduce raw value to result.value before import",
    "variant": "artifact: reduce raw variant to result.variant before import",
    "reason": "artifact: reduce raw reason to result.reason before import",
    "error_code": "artifact: reduce raw error_code to result.error_code before import",
    "error_message": "artifact: reduce raw error_message to result.error_message before import",
    "flag_metadata": "artifact: inline flag metadata is out of scope for EvaluationDetails v1",
    "provider": "artifact: provider identity is discovery-only and out of scope for v1",
    "provider_config": "artifact: provider configuration is out of scope for v1",
    "defined_flags": "artifact: flag definitions are discovery-only and out of scope for v1",
    "flag_definition": "artifact: flag definition import is out of scope for v1",
    "default_value": "artifact: caller-side default_value is discovery-only and out of scope for v1",
    "evaluation_context": "artifact: evaluation context is discovery-only and out of scope for v1",
    "targeting_key": "artifact: targeting identifiers are out of scope for v1",
    "targeting_rules": "artifact: targeting rules are out of scope for v1",
    "segments": "artifact: segment definitions are out of scope for v1",
    "rollout": "artifact: rollout configuration is out of scope for v1",
    "hooks": "artifact: hook state is out of scope for v1",
    "telemetry": "artifact: telemetry envelopes are out of scope for v1",
    "details": "artifact: detail arrays or wrappers are out of scope for one EvaluationDetails artifact",
    "results": "artifact: result arrays are out of scope for one EvaluationDetails artifact",
    "flags": "artifact: bulk flag state is out of scope for v1",
    "provider_metadata": "artifact: provider metadata is out of scope for v1",
}
TOP_LEVEL_KEYS = set(REQUIRED_KEYS) | OPTIONAL_TOP_LEVEL_KEYS | set(FORBIDDEN_TOP_LEVEL_KEY_MESSAGES)
RESULT_KEYS = {"value", "variant", "reason", "error_code", "error_message"}
MAX_FLAG_KEY_LENGTH = 200
MAX_REASON_LENGTH = 80
MAX_ERROR_MESSAGE_LENGTH = 240


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one OpenFeature EvaluationDetails artifact into an Assay-shaped envelope."
    )
    parser.add_argument("input", type=Path, help="OpenFeature artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-openfeature-<stem>.",
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


def _validate_non_empty_string(value: Any, field_name: str, max_length: int) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"artifact: {field_name} must be a non-empty string")
    stripped = value.strip()
    if len(stripped) > max_length:
        raise ValueError(f"artifact: {field_name} must be at most {max_length} characters")
    if "\n" in stripped or "\r" in stripped:
        raise ValueError(f"artifact: {field_name} must be a single-line string")
    return stripped


def _validate_optional_string(value: Any, field_name: str, max_length: int) -> str | None:
    if value is None:
        return None
    return _validate_non_empty_string(value, field_name, max_length)


def _validate_flag_key(value: Any) -> str:
    return _validate_non_empty_string(value, "flag_key", MAX_FLAG_KEY_LENGTH)


def _validate_boolean_value(value: Any) -> bool:
    if not isinstance(value, bool):
        raise ValueError("artifact: result.value must be a boolean for EvaluationDetails v1")
    return value


def _validate_metadata_ref(value: Any) -> str:
    return _validate_non_empty_string(value, "flag_metadata_ref", 160)


def _validate_result(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError("artifact: result must be an object")
    if not value:
        raise ValueError("artifact: result must be a non-empty object")

    unknown = set(value) - RESULT_KEYS
    if unknown:
        raise ValueError(f"artifact: unsupported result keys: {', '.join(sorted(unknown))}")

    if "value" not in value:
        raise ValueError("artifact: result.value is required")

    normalized: dict[str, Any] = {"value": _validate_boolean_value(value["value"])}
    for key in ("variant", "reason", "error_code", "error_message"):
        if key not in value:
            continue
        max_length = MAX_ERROR_MESSAGE_LENGTH if key == "error_message" else MAX_REASON_LENGTH
        optional_value = _validate_optional_string(value[key], f"result.{key}", max_length)
        if optional_value is not None:
            normalized[key] = optional_value

    if "error_code" in normalized and "error_message" not in normalized:
        raise ValueError("artifact: result.error_message is required when result.error_code is present")

    return normalized


def _raise_on_forbidden_top_level_keys(record: dict[str, Any]) -> None:
    present = [key for key in sorted(record) if key in FORBIDDEN_TOP_LEVEL_KEY_MESSAGES]
    if not present:
        return
    if len(present) == 1:
        raise ValueError(FORBIDDEN_TOP_LEVEL_KEY_MESSAGES[present[0]])
    details = "; ".join(FORBIDDEN_TOP_LEVEL_KEY_MESSAGES[key] for key in present)
    raise ValueError(
        f"artifact: forbidden top-level keys found: {', '.join(present)} ({details})"
    )


def _normalized_record(record: dict[str, Any]) -> dict[str, Any]:
    _raise_on_forbidden_top_level_keys(record)

    missing = [key for key in REQUIRED_KEYS if key not in record]
    if missing:
        raise ValueError(f"artifact: missing required keys: {', '.join(missing)}")

    unknown = set(record) - TOP_LEVEL_KEYS
    if unknown:
        raise ValueError(f"artifact: unsupported top-level keys: {', '.join(sorted(unknown))}")

    if record["schema"] != EXTERNAL_SCHEMA:
        raise ValueError(f"artifact: expected schema {EXTERNAL_SCHEMA}, got {record['schema']}")
    if record["framework"] != "openfeature":
        raise ValueError("artifact: framework must be openfeature")
    if record["surface"] != "evaluation_details":
        raise ValueError("artifact: surface must be evaluation_details")
    if record["target_kind"] != "feature_flag":
        raise ValueError("artifact: target_kind must be feature_flag")

    normalized = {
        "schema": EXTERNAL_SCHEMA,
        "framework": "openfeature",
        "surface": "evaluation_details",
        "target_kind": "feature_flag",
        "flag_key": _validate_flag_key(record["flag_key"]),
        "result": _validate_result(record["result"]),
    }
    if "flag_metadata_ref" in record:
        normalized["flag_metadata_ref"] = _validate_metadata_ref(record["flag_metadata_ref"])
    return normalized


def _build_event(normalized: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    data = {
        "external_system": "openfeature",
        "external_surface": "evaluation-details",
        "external_schema": EXTERNAL_SCHEMA,
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

    assay_run_id = args.assay_run_id or f"import-openfeature-{args.input.stem}"
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
