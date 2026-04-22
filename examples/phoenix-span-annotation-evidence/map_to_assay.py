"""Map a frozen Phoenix span annotation artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any, Optional


PLACEHOLDER_EVENT_TYPE = "example.placeholder.phoenix-span-annotation"
PLACEHOLDER_SOURCE = "urn:example:assay:external:phoenix:span-annotation"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "phoenix.span-annotation.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "entity_kind",
    "entity_id_ref",
    "annotation_name",
    "result",
    "timestamp",
)
OPTIONAL_KEYS = {
    "annotator_kind",
    "identifier",
    "metadata_ref",
}
FORBIDDEN_TOP_LEVEL_KEY_MESSAGES = {
    "annotations": "artifact: batch annotations are out of scope for single-annotation evidence",
    "metadata": "artifact: raw metadata is out of scope for single-annotation evidence",
    "span_ids": "artifact: multiple span targets are out of scope for single-annotation evidence",
    "span_id": "artifact: use entity_id_ref in the reduced artifact, not raw span_id",
    "name": "artifact: use annotation_name in the reduced artifact, not raw name",
    "id": "artifact: raw annotation id is out of scope for single-annotation evidence",
    "source": "artifact: raw source is out of scope for single-annotation evidence",
    "user_id": "artifact: raw user_id is out of scope for single-annotation evidence",
    "created_at": "artifact: raw created_at is out of scope for reduced single-annotation evidence",
    "updated_at": "artifact: raw updated_at must be reduced to timestamp before import",
}
TOP_LEVEL_KEYS = set(REQUIRED_KEYS) | OPTIONAL_KEYS | set(FORBIDDEN_TOP_LEVEL_KEY_MESSAGES)
ALLOWED_ANNOTATOR_KINDS = {"HUMAN", "LLM", "CODE"}
RESULT_KEYS = {"label", "score", "explanation"}
MAX_REF_LENGTH = 120
MAX_ANNOTATION_NAME_LENGTH = 80
MAX_LABEL_LENGTH = 80
MAX_EXPLANATION_LENGTH = 280


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one Phoenix span annotation artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="Phoenix artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-phoenix-<stem>.",
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
    # This sample keeps the fixture corpus inside the same deterministic JSON
    # subset the other interop examples use. It is not a full RFC 8785 / JCS
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
    if ref.startswith(("http://", "https://")):
        raise ValueError(f"{line_label}: {field_name} must be an opaque id, not a URL")
    if len(ref) > MAX_REF_LENGTH:
        raise ValueError(f"{line_label}: {field_name} must be at most {MAX_REF_LENGTH} characters")
    return ref


def _validate_annotation_name(value: Any, line_label: str) -> str:
    name = _validate_non_empty_string(value, line_label, "annotation_name")
    if len(name) > MAX_ANNOTATION_NAME_LENGTH:
        raise ValueError(
            f"{line_label}: annotation_name must be at most {MAX_ANNOTATION_NAME_LENGTH} characters"
        )
    return name


def _validate_label(value: Any, line_label: str) -> str:
    label = _validate_non_empty_string(value, line_label, "result.label")
    if len(label) > MAX_LABEL_LENGTH:
        raise ValueError(f"{line_label}: result.label must be at most {MAX_LABEL_LENGTH} characters")
    return label


def _validate_explanation(value: Any, line_label: str) -> str:
    explanation = _validate_non_empty_string(value, line_label, "result.explanation")
    if len(explanation) > MAX_EXPLANATION_LENGTH:
        raise ValueError(
            f"{line_label}: result.explanation must be a short string of at most {MAX_EXPLANATION_LENGTH} characters"
        )
    return explanation


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
    if "label" in value:
        normalized["label"] = _validate_label(value["label"], line_label)
    if "score" in value:
        normalized["score"] = _validate_score(value["score"], line_label)
    if "explanation" in value:
        normalized["explanation"] = _validate_explanation(value["explanation"], line_label)

    if "label" not in normalized and "score" not in normalized:
        raise ValueError(f"{line_label}: result must include at least one of label or score")

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
    if record["framework"] != "phoenix":
        raise ValueError("artifact: framework must be phoenix")
    if record["surface"] != "span_annotation":
        raise ValueError("artifact: surface must be span_annotation")
    if record["entity_kind"] != "span":
        raise ValueError("artifact: entity_kind must be span")

    normalized = {
        "schema": EXTERNAL_SCHEMA,
        "framework": "phoenix",
        "surface": "span_annotation",
        "entity_kind": "span",
        "entity_id_ref": _validate_short_ref(record["entity_id_ref"], "artifact", "entity_id_ref"),
        "annotation_name": _validate_annotation_name(record["annotation_name"], "artifact"),
        "result": _validate_result(record["result"], "artifact"),
        "timestamp": _parse_rfc3339_utc(str(record["timestamp"])),
    }

    if "annotator_kind" in record:
        annotator_kind = _validate_non_empty_string(record["annotator_kind"], "artifact", "annotator_kind")
        if annotator_kind not in ALLOWED_ANNOTATOR_KINDS:
            allowed = ", ".join(sorted(ALLOWED_ANNOTATOR_KINDS))
            raise ValueError(f"artifact: annotator_kind must be one of: {allowed}")
        normalized["annotator_kind"] = annotator_kind

    if "identifier" in record:
        normalized["identifier"] = _validate_short_ref(record["identifier"], "artifact", "identifier")

    if "metadata_ref" in record:
        normalized["metadata_ref"] = _validate_short_ref(record["metadata_ref"], "artifact", "metadata_ref")

    return normalized


def _build_event(normalized: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    data = {
        "external_system": "phoenix",
        "external_surface": "span-annotation",
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

    assay_run_id = args.assay_run_id or f"import-phoenix-{args.input.stem}"
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
