"""Map a frozen Promptfoo assertion result artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any


PLACEHOLDER_EVENT_TYPE = "example.placeholder.promptfoo-assertion-result"
PLACEHOLDER_SOURCE = "urn:example:assay:external:promptfoo:assertion-result"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "promptfoo.assertion-grading-result.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "target_kind",
    "assertion_type",
    "result",
)
FORBIDDEN_TOP_LEVEL_KEY_MESSAGES = {
    "pass": "artifact: reduce raw pass to result.pass before import",
    "score": "artifact: reduce raw score to result.score before import",
    "reason": "artifact: reduce raw reason to result.reason before import",
    "prompt": "artifact: raw prompt is discovery-only and out of scope for v1 import",
    "output": "artifact: raw output is discovery-only and out of scope for v1 import",
    "expected": "artifact: raw expected value is discovery-only and out of scope for v1 import",
    "vars": "artifact: raw vars are discovery-only and out of scope for v1 import",
    "assertion": "artifact: raw assertion config is out of scope for one surfaced result",
    "assertions": "artifact: assertion config arrays are out of scope for v1",
    "componentResults": "artifact: component result arrays are wrapper context, not v1 artifact shape",
    "gradingResult": "artifact: full Promptfoo gradingResult wrappers are out of scope for v1",
    "namedScores": "artifact: named score maps are out of scope for one deterministic assertion",
    "tokensUsed": "artifact: token usage is out of scope for one deterministic assertion",
    "tokenUsage": "artifact: token usage is out of scope for one deterministic assertion",
    "provider": "artifact: provider metadata is out of scope for v1",
    "providers": "artifact: provider arrays are out of scope for v1",
    "response": "artifact: provider response bodies are out of scope for v1",
    "success": "artifact: test-case success must not be imported as assertion pass",
    "failureReason": "artifact: test-case failureReason is out of scope for v1",
    "latencyMs": "artifact: latency is out of scope for v1",
    "cost": "artifact: cost is out of scope for v1",
    "promptIdx": "artifact: Promptfoo prompt indexes are not stable v1 target ids",
    "testIdx": "artifact: Promptfoo test indexes are not stable v1 target ids",
    "testCase": "artifact: raw Promptfoo test case wrappers are out of scope for v1",
    "promptId": "artifact: Promptfoo prompt ids are out of scope for v1",
    "prompts": "artifact: Promptfoo prompt arrays are out of scope for v1",
    "tests": "artifact: Promptfoo test arrays are out of scope for v1",
    "outputs": "artifact: Promptfoo output arrays are out of scope for v1",
    "results": "artifact: full Promptfoo result arrays are out of scope for v1",
    "stats": "artifact: Promptfoo stats are out of scope for one assertion result",
    "config": "artifact: Promptfoo config exports are out of scope for v1",
    "evalId": "artifact: Promptfoo eval ids are out of scope for one assertion result",
    "shareableUrl": "artifact: Promptfoo sharing metadata is out of scope for v1",
    "metadata": "artifact: Promptfoo metadata wrappers are out of scope for v1",
    "error": "artifact: Promptfoo row-level errors are out of scope for v1",
}
TOP_LEVEL_KEYS = set(REQUIRED_KEYS) | set(FORBIDDEN_TOP_LEVEL_KEY_MESSAGES)
RESULT_KEYS = {"pass", "score", "reason"}
MAX_ASSERTION_TYPE_LENGTH = 80
MAX_REASON_LENGTH = 240


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one Promptfoo assertion result artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="Promptfoo artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-promptfoo-<stem>.",
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


def _validate_assertion_type(value: Any) -> str:
    assertion_type = _validate_non_empty_string(value, "assertion_type")
    if len(assertion_type) > MAX_ASSERTION_TYPE_LENGTH:
        raise ValueError(
            f"artifact: assertion_type must be at most {MAX_ASSERTION_TYPE_LENGTH} characters"
        )
    if assertion_type != "equals":
        raise ValueError("artifact: assertion_type must be equals for Promptfoo v1")
    return assertion_type


def _validate_pass(value: Any) -> bool:
    if not isinstance(value, bool):
        raise ValueError("artifact: result.pass must be a boolean assertion outcome")
    return value


def _validate_score(value: Any) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise ValueError("artifact: result.score must be the discovered integer score shape")
    if value not in {0, 1}:
        raise ValueError("artifact: result.score must be 0 or 1 for Promptfoo equals v1")
    return value


def _validate_reason(value: Any) -> str:
    reason = _validate_non_empty_string(value, "result.reason")
    if len(reason) > MAX_REASON_LENGTH:
        raise ValueError(f"artifact: result.reason must be at most {MAX_REASON_LENGTH} characters")
    if "\n" in reason or "\r" in reason:
        raise ValueError("artifact: result.reason must stay single-line for Promptfoo v1")
    return reason


def _validate_result(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError("artifact: result must be an object")
    if not value:
        raise ValueError("artifact: result must be a non-empty object")

    unknown = set(value) - RESULT_KEYS
    if unknown:
        raise ValueError(f"artifact: unsupported result keys: {', '.join(sorted(unknown))}")

    missing = [key for key in ("pass", "score") if key not in value]
    if missing:
        raise ValueError(f"artifact: missing result keys: {', '.join(missing)}")

    result = {
        "pass": _validate_pass(value["pass"]),
        "score": _validate_score(value["score"]),
    }
    if "reason" in value:
        result["reason"] = _validate_reason(value["reason"])
    return result


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
    if record["framework"] != "promptfoo":
        raise ValueError("artifact: framework must be promptfoo")
    if record["surface"] != "assertion_grading_result":
        raise ValueError("artifact: surface must be assertion_grading_result")
    if record["target_kind"] != "promptfoo_output_assertion":
        raise ValueError("artifact: target_kind must be promptfoo_output_assertion")

    return {
        "schema": EXTERNAL_SCHEMA,
        "framework": "promptfoo",
        "surface": "assertion_grading_result",
        "target_kind": "promptfoo_output_assertion",
        "assertion_type": _validate_assertion_type(record["assertion_type"]),
        "result": _validate_result(record["result"]),
    }


def _build_event(normalized: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    data = {
        "external_system": "promptfoo",
        "external_surface": "assertion-grading-result",
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

    assay_run_id = args.assay_run_id or f"import-promptfoo-{args.input.stem}"
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
