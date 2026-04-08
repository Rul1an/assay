"""Map a frozen Agno accuracy-eval artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any


PLACEHOLDER_EVENT_TYPE = "example.placeholder.agno-accuracy-eval"
PLACEHOLDER_SOURCE = "urn:example:assay:external:agno:accuracy-eval"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "agno.accuracy-eval.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "eval_type",
    "eval_name",
    "timestamp",
    "outcome",
    "num_iterations",
    "scores",
    "avg_score",
)
OPTIONAL_KEYS = {
    "threshold",
    "input_label",
    "expected_output_ref",
    "guidelines_ref",
    "agent_ref",
}
ALLOWED_OUTCOMES = {"passed", "failed"}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one Agno accuracy artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="Agno accuracy artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-agno-<stem>.",
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


def _validate_non_negative_int(value: Any, line_label: str, field_name: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool):
        raise ValueError(f"{line_label}: {field_name} must be an integer")
    if value < 0:
        raise ValueError(f"{line_label}: {field_name} must be >= 0")
    return value


def _validate_optional_ref(value: Any, line_label: str, field_name: str) -> None:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{line_label}: {field_name} must be a non-empty string")


def _validate_scores(scores: Any, line_label: str) -> list[int]:
    if not isinstance(scores, list) or not scores:
        raise ValueError(f"{line_label}: scores must be a non-empty list")
    normalized: list[int] = []
    for index, value in enumerate(scores):
        score = _validate_non_negative_int(value, line_label, f"scores[{index}]")
        if score > 100:
            raise ValueError(f"{line_label}: scores[{index}] must be <= 100")
        normalized.append(score)
    return normalized


def _expected_average(scores: list[int]) -> int:
    return sum(scores) // len(scores)


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
    if record["framework"] != "agno":
        raise ValueError(f"{line_label}: framework must be agno")
    if record["surface"] != "accuracy_eval":
        raise ValueError(f"{line_label}: surface must be accuracy_eval")
    if record["eval_type"] != "accuracy":
        raise ValueError(f"{line_label}: eval_type must be accuracy")
    if not isinstance(record["eval_name"], str) or not record["eval_name"].strip():
        raise ValueError(f"{line_label}: eval_name must be a non-empty string")
    if not isinstance(record["outcome"], str) or record["outcome"] not in ALLOWED_OUTCOMES:
        allowed = ", ".join(sorted(ALLOWED_OUTCOMES))
        raise ValueError(f"{line_label}: outcome must be one of: {allowed}")

    num_iterations = _validate_non_negative_int(
        record["num_iterations"], line_label, "num_iterations"
    )
    if num_iterations <= 0:
        raise ValueError(f"{line_label}: num_iterations must be > 0")

    scores = _validate_scores(record["scores"], line_label)
    if len(scores) != num_iterations:
        raise ValueError(f"{line_label}: len(scores) must equal num_iterations")

    avg_score = _validate_non_negative_int(record["avg_score"], line_label, "avg_score")
    if avg_score > 100:
        raise ValueError(f"{line_label}: avg_score must be <= 100")
    if avg_score != _expected_average(scores):
        raise ValueError(f"{line_label}: avg_score must equal the integer average of scores")

    if "threshold" in record:
        threshold = _validate_non_negative_int(record["threshold"], line_label, "threshold")
        if threshold > 100:
            raise ValueError(f"{line_label}: threshold must be <= 100")
        expected_outcome = "passed" if avg_score >= threshold else "failed"
        if record["outcome"] != expected_outcome:
            raise ValueError(
                f"{line_label}: outcome must match avg_score relative to threshold when threshold is present"
            )

    for field in ("input_label", "expected_output_ref", "guidelines_ref", "agent_ref"):
        if field in record:
            _validate_optional_ref(record[field], line_label, field)


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
        "external_system": "agno",
        "external_surface": "accuracy-eval",
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
    assay_run_id = args.assay_run_id or f"import-agno-{args.input.stem}"

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
