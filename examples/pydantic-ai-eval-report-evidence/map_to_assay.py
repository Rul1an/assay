"""Map a frozen pydantic-ai eval-report artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any


PLACEHOLDER_EVENT_TYPE = "example.placeholder.pydantic-ai-evaluation-report"
PLACEHOLDER_SOURCE = "urn:example:assay:external:pydantic-ai:evaluation-report"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "pydantic-evals.evaluation-report.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "dataset_name",
    "experiment_name",
    "report_id",
    "timestamp",
    "outcome",
    "summary",
    "case_results",
)
OPTIONAL_KEYS = {"duration_ms", "trace_ref"}
ALLOWED_OUTCOMES = {"passed", "failed"}
ALLOWED_CASE_STATUSES = {"passed", "failed"}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one pydantic-ai eval-report artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="pydantic-ai eval-report artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-pydantic-ai-<stem>.",
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


def _validate_non_negative_int(value: Any, line_label: str, field_name: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool):
        raise ValueError(f"{line_label}: {field_name} must be an integer")
    if value < 0:
        raise ValueError(f"{line_label}: {field_name} must be >= 0")
    return value


def _validate_string_map(value: Any, line_label: str, field_name: str) -> None:
    if not isinstance(value, dict):
        raise ValueError(f"{line_label}: {field_name} must be a JSON object")
    for key, nested in value.items():
        if not isinstance(key, str) or not key.strip():
            raise ValueError(f"{line_label}: {field_name} keys must be non-empty strings")
        if not isinstance(nested, str) or not nested.strip():
            raise ValueError(f"{line_label}: {field_name}.{key} must be a non-empty string")


def _validate_boolean_map(value: Any, line_label: str, field_name: str) -> None:
    if not isinstance(value, dict):
        raise ValueError(f"{line_label}: {field_name} must be a JSON object")
    for key, nested in value.items():
        if not isinstance(key, str) or not key.strip():
            raise ValueError(f"{line_label}: {field_name} keys must be non-empty strings")
        if not isinstance(nested, bool):
            raise ValueError(f"{line_label}: {field_name}.{key} must be a boolean")


def _validate_scores(scores: Any, line_label: str) -> None:
    if not isinstance(scores, dict) or not scores:
        raise ValueError(f"{line_label}: scores must be a non-empty JSON object")
    for key, value in scores.items():
        if not isinstance(key, str) or not key.strip():
            raise ValueError(f"{line_label}: scores keys must be non-empty strings")
        points = _validate_non_negative_int(value, line_label, f"scores.{key}")
        if points > 100:
            raise ValueError(f"{line_label}: scores.{key} must be <= 100")


def _validate_summary(summary: Any, line_label: str) -> None:
    if not isinstance(summary, dict):
        raise ValueError(f"{line_label}: summary must be a JSON object")
    required = {"case_count", "pass_count", "fail_count"}
    optional = {"average_score"}
    missing = [key for key in required if key not in summary]
    if missing:
        joined = ", ".join(sorted(missing))
        raise ValueError(f"{line_label}: summary missing required keys: {joined}")
    unknown = set(summary) - required - optional
    if unknown:
        joined = ", ".join(sorted(unknown))
        raise ValueError(f"{line_label}: summary contains unsupported keys: {joined}")

    case_count = _validate_non_negative_int(summary["case_count"], line_label, "summary.case_count")
    pass_count = _validate_non_negative_int(summary["pass_count"], line_label, "summary.pass_count")
    fail_count = _validate_non_negative_int(summary["fail_count"], line_label, "summary.fail_count")
    if case_count != pass_count + fail_count:
        raise ValueError(f"{line_label}: summary.case_count must equal pass_count + fail_count")
    if "average_score" in summary:
        average_score = _validate_non_negative_int(
            summary["average_score"], line_label, "summary.average_score"
        )
        if average_score > 100:
            raise ValueError(f"{line_label}: summary.average_score must be <= 100")


def _validate_case_results(case_results: Any, line_label: str) -> tuple[int, int]:
    if not isinstance(case_results, list) or not case_results:
        raise ValueError(f"{line_label}: case_results must be a non-empty list")

    pass_count = 0
    fail_count = 0
    seen_ids: set[str] = set()
    for index, case_result in enumerate(case_results):
        item_label = f"{line_label}: case_results[{index}]"
        if not isinstance(case_result, dict):
            raise ValueError(f"{item_label} must be a JSON object")
        required = {"case_id", "status", "scores"}
        optional = {"assertions", "labels"}
        missing = [key for key in required if key not in case_result]
        if missing:
            joined = ", ".join(sorted(missing))
            raise ValueError(f"{item_label} missing required keys: {joined}")
        unknown = set(case_result) - required - optional
        if unknown:
            joined = ", ".join(sorted(unknown))
            raise ValueError(f"{item_label} contains unsupported keys: {joined}")

        case_id = case_result["case_id"]
        if not isinstance(case_id, str) or not case_id.strip():
            raise ValueError(f"{item_label}: case_id must be a non-empty string")
        if case_id in seen_ids:
            raise ValueError(f"{item_label}: duplicate case_id {case_id}")
        seen_ids.add(case_id)

        status = case_result["status"]
        if not isinstance(status, str) or status not in ALLOWED_CASE_STATUSES:
            allowed = ", ".join(sorted(ALLOWED_CASE_STATUSES))
            raise ValueError(f"{item_label}: status must be one of: {allowed}")
        if status == "passed":
            pass_count += 1
        else:
            fail_count += 1

        _validate_scores(case_result["scores"], item_label)
        if "assertions" in case_result:
            _validate_boolean_map(case_result["assertions"], item_label, "assertions")
        if "labels" in case_result:
            _validate_string_map(case_result["labels"], item_label, "labels")

    return pass_count, fail_count


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
    if record["framework"] != "pydantic-ai":
        raise ValueError(f"{line_label}: framework must be pydantic-ai")
    if record["surface"] != "evaluation_report":
        raise ValueError(f"{line_label}: surface must be evaluation_report")
    for key in ("dataset_name", "experiment_name", "report_id"):
        if not isinstance(record[key], str) or not record[key].strip():
            raise ValueError(f"{line_label}: {key} must be a non-empty string")
    if not isinstance(record["outcome"], str) or record["outcome"] not in ALLOWED_OUTCOMES:
        allowed = ", ".join(sorted(ALLOWED_OUTCOMES))
        raise ValueError(f"{line_label}: outcome must be one of: {allowed}")

    _validate_summary(record["summary"], line_label)
    pass_count, fail_count = _validate_case_results(record["case_results"], line_label)
    summary = record["summary"]
    if summary["case_count"] != len(record["case_results"]):
        raise ValueError(f"{line_label}: summary.case_count must equal len(case_results)")
    if summary["pass_count"] != pass_count or summary["fail_count"] != fail_count:
        raise ValueError(f"{line_label}: summary pass/fail counts must match case_results")
    expected_outcome = "passed" if fail_count == 0 else "failed"
    if record["outcome"] != expected_outcome:
        raise ValueError(f"{line_label}: outcome must match aggregated case_results status")

    if "duration_ms" in record:
        _validate_non_negative_int(record["duration_ms"], line_label, "duration_ms")
    if "trace_ref" in record:
        if not isinstance(record["trace_ref"], str) or not record["trace_ref"].strip():
            raise ValueError(f"{line_label}: trace_ref must be a non-empty string")


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
        "external_system": "pydantic-ai",
        "external_surface": "evaluation-report",
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
        import_time = _parse_rfc3339_utc(args.import_time)
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc
    assay_run_id = args.assay_run_id or f"import-pydantic-ai-{args.input.stem}"

    try:
        record = json.loads(args.input.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise SystemExit(f"artifact: invalid JSON: {exc.msg}") from exc
    if not isinstance(record, dict):
        raise SystemExit("artifact: expected a JSON object")
    try:
        _validate_record(record, "artifact")
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc

    try:
        event = _build_event(record, assay_run_id, import_time)
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", encoding="utf-8") as handle:
        handle.write(_canonical_json(event))
        handle.write("\n")

    print(f"Wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
