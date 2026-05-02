"""Map one reduced pydantic_evals case-result artifact into an Assay-shaped envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any


PLACEHOLDER_EVENT_TYPE = "example.placeholder.pydantic-evals-report-case-result"
PLACEHOLDER_SOURCE = "urn:example:assay:external:pydantic-evals:report-case-result"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "pydantic-evals.report-case-result.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "case_name",
    "results",
    "timestamp",
)
OPTIONAL_KEYS = {"source_case_name", "source_ref"}
ALLOWED_RESULT_KINDS = {"assertion", "score"}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Map one reduced pydantic_evals case-result artifact into an "
            "Assay-shaped placeholder envelope."
        )
    )
    parser.add_argument(
        "input",
        type=Path,
        help="Reduced pydantic_evals case-result artifact to read.",
    )
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
        help="Optional Assay run id override. Defaults to import-pydantic-evals-<stem>.",
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
    # small JSON subset the other interop samples use. It is not a full RFC
    # 8785 / JCS implementation for arbitrary JSON inputs.
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


def _validate_finite_number(value: Any, line_label: str, field_name: str) -> None:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"{line_label}: {field_name} must be a number")
    if not math.isfinite(float(value)):
        raise ValueError(f"{line_label}: {field_name} must be finite")


def _validate_result_reason(result: dict[str, Any], item_label: str) -> None:
    if "reason" not in result:
        return
    if not isinstance(result["reason"], str) or not result["reason"].strip():
        raise ValueError(f"{item_label}: reason must be a non-empty string")


def _validate_results(results: Any, line_label: str) -> None:
    if not isinstance(results, list) or not results:
        raise ValueError(f"{line_label}: results must be a non-empty list")

    seen_results: set[tuple[str, str]] = set()
    for index, result in enumerate(results):
        item_label = f"{line_label}: results[{index}]"
        if not isinstance(result, dict):
            raise ValueError(f"{item_label} must be a JSON object")

        required = {"kind", "evaluator_name"}
        missing = [key for key in required if key not in result]
        if missing:
            joined = ", ".join(sorted(missing))
            raise ValueError(f"{item_label} missing required keys: {joined}")

        kind = result["kind"]
        if not isinstance(kind, str) or kind not in ALLOWED_RESULT_KINDS:
            allowed = ", ".join(sorted(ALLOWED_RESULT_KINDS))
            raise ValueError(f"{item_label}: kind must be one of: {allowed}")
        evaluator_name = result["evaluator_name"]
        if not isinstance(evaluator_name, str) or not evaluator_name.strip():
            raise ValueError(f"{item_label}: evaluator_name must be a non-empty string")

        result_key = (kind, evaluator_name)
        if result_key in seen_results:
            raise ValueError(f"{item_label}: duplicate {kind} result for {evaluator_name}")
        seen_results.add(result_key)

        _validate_result_reason(result, item_label)
        if kind == "assertion":
            allowed_keys = required | {"passed", "reason"}
            if "passed" not in result:
                raise ValueError(f"{item_label} missing required keys: passed")
            if not isinstance(result["passed"], bool):
                raise ValueError(f"{item_label}: passed must be a boolean")
        else:
            allowed_keys = required | {"score", "reason"}
            if "score" not in result:
                raise ValueError(f"{item_label} missing required keys: score")
            _validate_finite_number(result["score"], item_label, "score")

        unknown = set(result) - allowed_keys
        if unknown:
            joined = ", ".join(sorted(unknown))
            raise ValueError(f"{item_label} contains unsupported keys: {joined}")


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
    if record["framework"] != "pydantic_evals":
        raise ValueError(f"{line_label}: framework must be pydantic_evals")
    if record["surface"] != "evaluation_report.cases.case_result":
        raise ValueError(
            f"{line_label}: surface must be evaluation_report.cases.case_result"
        )
    for key in ("case_name",):
        if not isinstance(record[key], str) or not record[key].strip():
            raise ValueError(f"{line_label}: {key} must be a non-empty string")
    for key in ("source_case_name", "source_ref"):
        if key in record and (not isinstance(record[key], str) or not record[key].strip()):
            raise ValueError(f"{line_label}: {key} must be a non-empty string")

    _validate_results(record["results"], line_label)


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
        "external_system": "pydantic_evals",
        "external_surface": "evaluation_report.cases.case_result",
        "external_schema": EXTERNAL_SCHEMA,
        "case_name": normalized["case_name"],
        "observed_export_time": normalized["timestamp"],
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
    assay_run_id = args.assay_run_id or f"import-pydantic-evals-{args.input.stem}"

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
