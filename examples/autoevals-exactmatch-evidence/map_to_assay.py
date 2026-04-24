"""Map a frozen AutoEvals ExactMatch artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any


PLACEHOLDER_EVENT_TYPE = "example.placeholder.autoevals-exactmatch"
PLACEHOLDER_SOURCE = "urn:example:assay:external:autoevals:exactmatch"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "autoevals.exactmatch-score.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "target_kind",
    "scorer_name",
    "result",
)
FORBIDDEN_TOP_LEVEL_KEY_MESSAGES = {
    "name": "artifact: reduce raw name to scorer_name before import",
    "score": "artifact: reduce raw score to result.score before import",
    "metadata": "artifact: raw metadata is out of scope for ExactMatch v1",
    "metadata_ref": "artifact: metadata_ref is out of scope until discovery proves a stable subset",
    "error": "artifact: raw error state is out of scope for successful ExactMatch score evidence",
    "output": "artifact: raw output is discovery-only and out of scope for v1 import",
    "expected": "artifact: raw expected is discovery-only and out of scope for v1 import",
    "input": "artifact: raw input is discovery-only and out of scope for v1 import",
    "outputs": "artifact: raw outputs are out of scope for one ExactMatch score",
    "reference_outputs": "artifact: raw reference_outputs are out of scope for one ExactMatch score",
    "scores": "artifact: score arrays are out of scope for one ExactMatch score",
    "results": "artifact: result arrays are out of scope for one ExactMatch score",
    "eval_results": "artifact: Braintrust or evaluator result bundles are out of scope for v1",
    "experiment_id": "artifact: Braintrust experiment wrappers are out of scope for v1",
    "dataset_id": "artifact: dataset identifiers are out of scope for v1",
    "scorer_config": "artifact: scorer configuration is out of scope for ExactMatch v1",
    "model": "artifact: model metadata is out of scope for ExactMatch v1",
    "prompt": "artifact: prompt metadata is out of scope for ExactMatch v1",
    "rubric": "artifact: rubric metadata is out of scope for ExactMatch v1",
    "context": "artifact: RAG context metadata is out of scope for ExactMatch v1",
    "provider": "artifact: provider metadata is out of scope for ExactMatch v1",
}
TOP_LEVEL_KEYS = set(REQUIRED_KEYS) | set(FORBIDDEN_TOP_LEVEL_KEY_MESSAGES)
RESULT_KEYS = {"score"}
MAX_SCORER_NAME_LENGTH = 80


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one AutoEvals ExactMatch artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="AutoEvals artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-autoevals-<stem>.",
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


def _validate_non_empty_string(value: Any, field_name: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"artifact: {field_name} must be a non-empty string")
    return value.strip()


def _validate_scorer_name(value: Any) -> str:
    scorer_name = _validate_non_empty_string(value, "scorer_name")
    if len(scorer_name) > MAX_SCORER_NAME_LENGTH:
        raise ValueError(
            f"artifact: scorer_name must be at most {MAX_SCORER_NAME_LENGTH} characters"
        )
    return scorer_name


def _validate_score(value: Any) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise ValueError("artifact: result.score must be the discovered integer score shape")
    if value not in {0, 1}:
        raise ValueError("artifact: result.score must be 0 or 1 for ExactMatch v1")
    return value


def _validate_result(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError("artifact: result must be an object")
    if not value:
        raise ValueError("artifact: result must be a non-empty object")

    unknown = set(value) - RESULT_KEYS
    if unknown:
        raise ValueError(f"artifact: unsupported result keys: {', '.join(sorted(unknown))}")

    if "score" not in value:
        raise ValueError("artifact: result.score is required")
    return {"score": _validate_score(value["score"])}


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
    if record["framework"] != "autoevals":
        raise ValueError("artifact: framework must be autoevals")
    if record["surface"] != "exactmatch_score":
        raise ValueError("artifact: surface must be exactmatch_score")
    if record["target_kind"] != "output_expected_pair":
        raise ValueError("artifact: target_kind must be output_expected_pair")

    return {
        "schema": EXTERNAL_SCHEMA,
        "framework": "autoevals",
        "surface": "exactmatch_score",
        "target_kind": "output_expected_pair",
        "scorer_name": _validate_scorer_name(record["scorer_name"]),
        "result": _validate_result(record["result"]),
    }


def _build_event(normalized: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    data = {
        "external_system": "autoevals",
        "external_surface": "exactmatch-score",
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

    assay_run_id = args.assay_run_id or f"import-autoevals-{args.input.stem}"
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
