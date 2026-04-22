"""Map a frozen LangWatch custom span evaluation artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any, Optional


PLACEHOLDER_EVENT_TYPE = "example.placeholder.langwatch-custom-span-evaluation"
PLACEHOLDER_SOURCE = "urn:example:assay:external:langwatch:custom-span-evaluation"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "langwatch.custom-span-evaluation.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "entity_kind",
    "entity_id_ref",
    "evaluation_name",
    "result",
)
OPTIONAL_KEYS = {
    "timestamp",
    "trace_id_ref",
    "sdk_language",
}
FORBIDDEN_TOP_LEVEL_KEY_MESSAGES = {
    "evaluations": "artifact: evaluation arrays are out of scope for single-evaluation evidence",
    "spans": "artifact: trace envelopes are out of scope for single-evaluation evidence",
    "trace_id": "artifact: use optional trace_id_ref in the reduced artifact, not raw trace_id",
    "span_id": "artifact: use entity_id_ref in the reduced artifact, not raw span_id",
    "parent_id": "artifact: reduce parent_id to entity_id_ref before import",
    "name": "artifact: use evaluation_name in the reduced artifact, not raw name",
    "type": "artifact: reduce raw type into the bounded surface field before import",
    "input": "artifact: raw input is out of scope for reduced single-evaluation evidence",
    "output": "artifact: raw output is out of scope for reduced single-evaluation evidence",
    "timestamps": "artifact: raw timestamps must be reduced before import",
    "metrics": "artifact: raw metrics are out of scope for the first evidence lane",
    "params": "artifact: raw params are out of scope for the first evidence lane",
    "project_id": "artifact: raw project_id is out of scope for the reduced artifact",
}
TOP_LEVEL_KEYS = set(REQUIRED_KEYS) | OPTIONAL_KEYS | set(FORBIDDEN_TOP_LEVEL_KEY_MESSAGES)
RESULT_KEYS = {"passed", "score", "label", "details"}
MAX_REF_LENGTH = 120
MAX_EVALUATION_NAME_LENGTH = 80
MAX_LABEL_LENGTH = 80
MAX_DETAILS_LENGTH = 280
ALLOWED_SDK_LANGUAGES = {"python"}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one LangWatch custom span evaluation artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="LangWatch artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-langwatch-<stem>.",
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
    if ref.startswith(("http://", "https://")):
        raise ValueError(f"{line_label}: {field_name} must be an opaque id, not a URL")
    if len(ref) > MAX_REF_LENGTH:
        raise ValueError(f"{line_label}: {field_name} must be at most {MAX_REF_LENGTH} characters")
    return ref


def _validate_evaluation_name(value: Any, line_label: str) -> str:
    name = _validate_non_empty_string(value, line_label, "evaluation_name")
    if len(name) > MAX_EVALUATION_NAME_LENGTH:
        raise ValueError(
            f"{line_label}: evaluation_name must be at most {MAX_EVALUATION_NAME_LENGTH} characters"
        )
    return name


def _validate_label(value: Any, line_label: str) -> str:
    label = _validate_non_empty_string(value, line_label, "result.label")
    if len(label) > MAX_LABEL_LENGTH:
        raise ValueError(f"{line_label}: result.label must be at most {MAX_LABEL_LENGTH} characters")
    return label


def _validate_details(value: Any, line_label: str) -> str:
    details = _validate_non_empty_string(value, line_label, "result.details")
    if len(details) > MAX_DETAILS_LENGTH:
        raise ValueError(
            f"{line_label}: result.details must be a short string of at most {MAX_DETAILS_LENGTH} characters"
        )
    return details


def _validate_score(value: Any, line_label: str) -> float | int:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"{line_label}: result.score must be a number")
    if not math.isfinite(float(value)):
        raise ValueError(f"{line_label}: result.score must be finite")
    return value


def _validate_result(value: Any, line_label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError(f"{line_label}: result must be an object")
    if not value:
        raise ValueError(f"{line_label}: result must be a non-empty object")

    unknown = set(value) - RESULT_KEYS
    if unknown:
        raise ValueError(f"{line_label}: unsupported result keys: {', '.join(sorted(unknown))}")

    normalized: dict[str, Any] = {}
    if "passed" in value:
        if not isinstance(value["passed"], bool):
            raise ValueError(f"{line_label}: result.passed must be a boolean")
        normalized["passed"] = value["passed"]
    if "score" in value:
        normalized["score"] = _validate_score(value["score"], line_label)
    if "label" in value:
        normalized["label"] = _validate_label(value["label"], line_label)
    if "details" in value:
        normalized["details"] = _validate_details(value["details"], line_label)

    if not {"passed", "score", "label"} & set(normalized):
        raise ValueError(f"{line_label}: result must include at least one of passed, score, or label")

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
    if record["framework"] != "langwatch":
        raise ValueError("artifact: framework must be langwatch")
    if record["surface"] != "custom_span_evaluation":
        raise ValueError("artifact: surface must be custom_span_evaluation")
    if record["entity_kind"] != "span":
        raise ValueError("artifact: entity_kind must be span")

    normalized = {
        "schema": EXTERNAL_SCHEMA,
        "framework": "langwatch",
        "surface": "custom_span_evaluation",
        "entity_kind": "span",
        "entity_id_ref": _validate_short_ref(record["entity_id_ref"], "artifact", "entity_id_ref"),
        "evaluation_name": _validate_evaluation_name(record["evaluation_name"], "artifact"),
        "result": _validate_result(record["result"], "artifact"),
    }

    if "timestamp" in record:
        normalized["timestamp"] = _parse_rfc3339_utc(str(record["timestamp"]))
    if "trace_id_ref" in record:
        normalized["trace_id_ref"] = _validate_short_ref(
            record["trace_id_ref"], "artifact", "trace_id_ref"
        )
    if "sdk_language" in record:
        sdk_language = _validate_non_empty_string(record["sdk_language"], "artifact", "sdk_language")
        if sdk_language not in ALLOWED_SDK_LANGUAGES:
            allowed = ", ".join(sorted(ALLOWED_SDK_LANGUAGES))
            raise ValueError(f"artifact: sdk_language must be one of: {allowed}")
        normalized["sdk_language"] = sdk_language

    return normalized


def _build_event(normalized: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    data = {
        "external_system": "langwatch",
        "external_surface": "custom-span-evaluation",
        "external_schema": EXTERNAL_SCHEMA,
        "observed": normalized,
    }
    if "timestamp" in normalized:
        data["observed_upstream_time"] = normalized["timestamp"]

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

    assay_run_id = args.assay_run_id or f"import-langwatch-{args.input.stem}"
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
