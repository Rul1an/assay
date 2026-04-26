"""Map a frozen Guardrails validation outcome artifact into an Assay-shaped envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any


PLACEHOLDER_EVENT_TYPE = "example.placeholder.guardrails-validation-outcome"
PLACEHOLDER_SOURCE = "urn:example:assay:external:guardrails:validation-outcome"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "guardrails.validation-outcome.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "target_kind",
    "validation_passed",
    "result",
)
OPTIONAL_TOP_LEVEL_KEYS = {"validator_name", "call_id_ref"}
FORBIDDEN_TOP_LEVEL_KEY_MESSAGES = {
    "outcome": "artifact: reduce raw outcome to result.outcome before import",
    "error_message": "artifact: reduce raw error_message to result.error before import",
    "errorMessage": "artifact: reduce raw errorMessage to result.error before import",
    "raw_llm_output": "artifact: raw LLM output is discovery-only and out of scope for v1",
    "raw_output": "artifact: raw output is discovery-only and out of scope for v1",
    "output": "artifact: raw validation value is discovery-only and out of scope for v1",
    "value": "artifact: raw validation value is discovery-only and out of scope for v1",
    "input": "artifact: raw validation input is discovery-only and out of scope for v1",
    "prompt": "artifact: prompt text is out of scope for v1",
    "prompt_text": "artifact: prompt text is out of scope for v1",
    "validated_output": "artifact: validated output is discovery-only and out of scope for v1",
    "corrected_output": "artifact: corrected output is discovery-only and out of scope for v1",
    "fix_value": "artifact: fix_value is discovery-only and out of scope for v1",
    "fixValue": "artifact: fixValue is discovery-only and out of scope for v1",
    "value_override": "artifact: value_override is discovery-only and out of scope for v1",
    "valueOverride": "artifact: valueOverride is discovery-only and out of scope for v1",
    "validated_chunk": "artifact: validated_chunk is discovery-only and out of scope for v1",
    "validatedChunk": "artifact: validatedChunk is discovery-only and out of scope for v1",
    "reask": "artifact: reask payloads are out of scope for v1",
    "guard_history": "artifact: guard history is out of scope for one validation outcome",
    "history": "artifact: guard history is out of scope for one validation outcome",
    "validator_logs": "artifact: validator logs are out of scope for v1",
    "logs": "artifact: validator logs are out of scope for v1",
    "metadata": "artifact: validator metadata is discovery-only and out of scope for v1",
    "metadata_ref": "artifact: metadata_ref is out of scope for Guardrails v1",
    "error_spans": "artifact: error spans are too rich for Guardrails v1",
    "errorSpans": "artifact: error spans are too rich for Guardrails v1",
    "errors": "artifact: error arrays are out of scope for one validation outcome",
    "results": "artifact: result arrays are out of scope for one validation outcome",
    "validation_results": "artifact: validation result arrays are out of scope for v1",
    "model": "artifact: model metadata is out of scope for v1",
    "provider": "artifact: provider metadata is out of scope for v1",
    "trace": "artifact: traces are out of scope for v1",
    "telemetry": "artifact: telemetry envelopes are out of scope for v1",
}
TOP_LEVEL_KEYS = (
    set(REQUIRED_KEYS) | OPTIONAL_TOP_LEVEL_KEYS | set(FORBIDDEN_TOP_LEVEL_KEY_MESSAGES)
)
RESULT_KEYS = {"outcome", "error"}
MAX_VALIDATOR_NAME_LENGTH = 120
MAX_CALL_REF_LENGTH = 160
MAX_ERROR_LENGTH = 240


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one Guardrails validation artifact into an Assay-shaped envelope."
    )
    parser.add_argument("input", type=Path, help="Guardrails artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-guardrails-<stem>.",
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


def _validate_boolean(value: Any, field_name: str) -> bool:
    if not isinstance(value, bool):
        raise ValueError(f"artifact: {field_name} must be a boolean")
    return value


def _validate_result(value: Any, validation_passed: bool) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError("artifact: result must be an object")
    if not value:
        raise ValueError("artifact: result must be a non-empty object")

    unknown = set(value) - RESULT_KEYS
    if unknown:
        raise ValueError(f"artifact: unsupported result keys: {', '.join(sorted(unknown))}")

    outcome = _validate_non_empty_string(value.get("outcome"), "result.outcome", 20)
    if outcome not in {"pass", "fail"}:
        raise ValueError("artifact: result.outcome must be pass or fail")

    if validation_passed and outcome != "pass":
        raise ValueError("artifact: validation_passed=true requires result.outcome=pass")
    if not validation_passed and outcome != "fail":
        raise ValueError("artifact: validation_passed=false requires result.outcome=fail")

    normalized: dict[str, Any] = {"outcome": outcome}
    error = _validate_optional_string(value.get("error"), "result.error", MAX_ERROR_LENGTH)
    if outcome == "pass" and error is not None:
        raise ValueError("artifact: result.error is only allowed for failed validation")
    if outcome == "fail" and error is None:
        raise ValueError("artifact: result.error is required for failed validation")
    if error is not None:
        normalized["error"] = error
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
    if record["framework"] != "guardrails-ai":
        raise ValueError("artifact: framework must be guardrails-ai")
    if record["surface"] != "validation_result":
        raise ValueError("artifact: surface must be validation_result")
    if record["target_kind"] != "validation_call":
        raise ValueError("artifact: target_kind must be validation_call")

    validation_passed = _validate_boolean(record["validation_passed"], "validation_passed")
    normalized = {
        "schema": EXTERNAL_SCHEMA,
        "framework": "guardrails-ai",
        "surface": "validation_result",
        "target_kind": "validation_call",
        "validation_passed": validation_passed,
        "result": _validate_result(record["result"], validation_passed),
    }
    if "validator_name" in record:
        normalized["validator_name"] = _validate_non_empty_string(
            record["validator_name"], "validator_name", MAX_VALIDATOR_NAME_LENGTH
        )
    if "call_id_ref" in record:
        normalized["call_id_ref"] = _validate_non_empty_string(
            record["call_id_ref"], "call_id_ref", MAX_CALL_REF_LENGTH
        )
    return normalized


def _build_event(normalized: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    data = {
        "external_system": "guardrails-ai",
        "external_surface": "validation-result",
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

    assay_run_id = args.assay_run_id or f"import-guardrails-{args.input.stem}"
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
