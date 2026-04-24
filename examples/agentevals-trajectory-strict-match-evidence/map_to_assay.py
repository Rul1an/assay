"""Map a frozen AgentEvals strict-match artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
from typing import Any


PLACEHOLDER_EVENT_TYPE = "example.placeholder.agentevals-trajectory-strict-match"
PLACEHOLDER_SOURCE = "urn:example:assay:external:agentevals:trajectory-strict-match"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "agentevals.trajectory-strict-match.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "target_kind",
    "evaluator_key",
    "result",
)
FORBIDDEN_TOP_LEVEL_KEY_MESSAGES = {
    "key": "artifact: reduce raw key to evaluator_key before import",
    "score": "artifact: reduce raw score to result.score before import",
    "comment": "artifact: reduce raw comment to result.comment before import",
    "metadata": "artifact: raw metadata is out of scope for the first returned-result lane",
    "outputs": "artifact: raw outputs are discovery-only and out of scope for v1 import",
    "reference_outputs": "artifact: raw reference_outputs are discovery-only and out of scope for v1 import",
    "trajectory": "artifact: raw trajectory payloads are out of scope for the first returned-result lane",
    "reference_trajectory": "artifact: raw reference trajectory payloads are out of scope for the first returned-result lane",
    "trajectory_match_mode": "artifact: evaluator configuration is out of scope for the first returned-result lane",
    "results": "artifact: result arrays are out of scope for single-result evidence",
    "evaluations": "artifact: evaluation arrays are out of scope for single-result evidence",
    "run_id": "artifact: LangSmith or run-level wrappers are out of scope for v1 import",
    "experiment_id": "artifact: experiment wrappers are out of scope for v1 import",
    "dataset_id": "artifact: dataset identifiers are out of scope for v1 import",
    "model": "artifact: model metadata is out of scope for the first returned-result lane",
    "prompt": "artifact: prompt metadata is out of scope for the first returned-result lane",
    "rubric": "artifact: rubric metadata is out of scope for the first returned-result lane",
}
TOP_LEVEL_KEYS = set(REQUIRED_KEYS) | set(FORBIDDEN_TOP_LEVEL_KEY_MESSAGES)
RESULT_KEYS = {"score", "comment"}
MAX_EVALUATOR_KEY_LENGTH = 80
MAX_COMMENT_LENGTH = 280


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one AgentEvals strict-match artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="AgentEvals artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-agentevals-<stem>.",
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


def _canonical_json(value: Any) -> str:
    return json.dumps(
        value,
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


def _validate_evaluator_key(value: Any) -> str:
    evaluator_key = _validate_non_empty_string(value, "evaluator_key")
    if len(evaluator_key) > MAX_EVALUATOR_KEY_LENGTH:
        raise ValueError(
            f"artifact: evaluator_key must be at most {MAX_EVALUATOR_KEY_LENGTH} characters"
        )
    return evaluator_key


def _validate_comment(value: Any) -> str:
    comment = _validate_non_empty_string(value, "result.comment")
    if "\n" in comment or "\r" in comment:
        raise ValueError("artifact: result.comment must stay single-line in v1")
    if len(comment) > MAX_COMMENT_LENGTH:
        raise ValueError(
            f"artifact: result.comment must be at most {MAX_COMMENT_LENGTH} characters"
        )
    return comment


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
    if not isinstance(value["score"], bool):
        raise ValueError("artifact: result.score must be a boolean")

    normalized = {"score": value["score"]}
    if "comment" in value:
        normalized["comment"] = _validate_comment(value["comment"])
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
    if record["framework"] != "agentevals":
        raise ValueError("artifact: framework must be agentevals")
    if record["surface"] != "trajectory_strict_match_result":
        raise ValueError("artifact: surface must be trajectory_strict_match_result")
    if record["target_kind"] != "trajectory":
        raise ValueError("artifact: target_kind must be trajectory")

    return {
        "schema": EXTERNAL_SCHEMA,
        "framework": "agentevals",
        "surface": "trajectory_strict_match_result",
        "target_kind": "trajectory",
        "evaluator_key": _validate_evaluator_key(record["evaluator_key"]),
        "result": _validate_result(record["result"]),
    }


def _build_event(normalized: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    data = {
        "external_system": "agentevals",
        "external_surface": "trajectory-strict-match-result",
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

    assay_run_id = args.assay_run_id or f"import-agentevals-{args.input.stem}"
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
