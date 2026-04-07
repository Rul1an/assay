"""Map a frozen mcp-agent token summary artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any


PLACEHOLDER_EVENT_TYPE = "example.placeholder.mcp-agent-token-summary"
PLACEHOLDER_SOURCE = "urn:example:assay:external:mcp-agent:token-summary"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "mcp-agent.token-summary.export.v1"
REQUIRED_KEYS = (
    "schema",
    "framework",
    "surface",
    "workflow_name",
    "run_id",
    "timestamp",
    "outcome",
    "token_summary",
)
OPTIONAL_KEYS = {"model_breakdown", "cost_estimate_usd", "tree_ref"}
ALLOWED_OUTCOMES = {"completed", "failed", "aborted"}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one mcp-agent token summary artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="mcp-agent token summary artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-mcp-agent-<stem>.",
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
    # This sample keeps the fixture corpus in the JCS-safe subset
    # (objects, arrays, strings, bools, null, and integer-valued numbers),
    # so deterministic sorted-key JSON matches the bytes Assay hashes today.
    # It is not a full RFC 8785 implementation for arbitrary JSON inputs.
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


def _validate_optional_non_negative_number(value: Any, line_label: str, field_name: str) -> None:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        raise ValueError(f"{line_label}: {field_name} must be a number")
    if not math.isfinite(value):
        raise ValueError(f"{line_label}: {field_name} must be finite")
    if value < 0:
        raise ValueError(f"{line_label}: {field_name} must be >= 0")


def _validate_token_summary(token_summary: Any, line_label: str) -> tuple[int, int, int]:
    if not isinstance(token_summary, dict):
        raise ValueError(f"{line_label}: token_summary must be a JSON object")
    required = {"total_tokens", "input_tokens", "output_tokens"}
    missing = [key for key in required if key not in token_summary]
    if missing:
        joined = ", ".join(sorted(missing))
        raise ValueError(f"{line_label}: token_summary missing required keys: {joined}")
    unknown = set(token_summary) - required
    if unknown:
        joined = ", ".join(sorted(unknown))
        raise ValueError(f"{line_label}: token_summary contains unsupported keys: {joined}")

    total_tokens = _validate_non_negative_int(
        token_summary["total_tokens"], line_label, "token_summary.total_tokens"
    )
    input_tokens = _validate_non_negative_int(
        token_summary["input_tokens"], line_label, "token_summary.input_tokens"
    )
    output_tokens = _validate_non_negative_int(
        token_summary["output_tokens"], line_label, "token_summary.output_tokens"
    )
    if total_tokens != input_tokens + output_tokens:
        raise ValueError(
            f"{line_label}: token_summary.total_tokens must equal input_tokens + output_tokens"
        )
    return total_tokens, input_tokens, output_tokens


def _validate_model_breakdown(
    model_breakdown: Any,
    summary_totals: tuple[int, int, int],
    line_label: str,
) -> None:
    if not isinstance(model_breakdown, list) or not model_breakdown:
        raise ValueError(f"{line_label}: model_breakdown must be a non-empty list")

    aggregate_total = 0
    aggregate_input = 0
    aggregate_output = 0
    allowed = {
        "model_name",
        "provider",
        "total_tokens",
        "input_tokens",
        "output_tokens",
        "cost_estimate_usd",
    }
    for index, item in enumerate(model_breakdown):
        item_label = f"{line_label}: model_breakdown[{index}]"
        if not isinstance(item, dict):
            raise ValueError(f"{item_label} must be a JSON object")
        missing = [key for key in ("model_name", "provider", "total_tokens", "input_tokens", "output_tokens") if key not in item]
        if missing:
            joined = ", ".join(missing)
            raise ValueError(f"{item_label} missing required keys: {joined}")
        unknown = set(item) - allowed
        if unknown:
            joined = ", ".join(sorted(unknown))
            raise ValueError(f"{item_label} contains unsupported keys: {joined}")
        if not isinstance(item["model_name"], str) or not item["model_name"].strip():
            raise ValueError(f"{item_label}: model_name must be a non-empty string")
        if not isinstance(item["provider"], str) or not item["provider"].strip():
            raise ValueError(f"{item_label}: provider must be a non-empty string")

        total_tokens = _validate_non_negative_int(
            item["total_tokens"], item_label, "total_tokens"
        )
        input_tokens = _validate_non_negative_int(
            item["input_tokens"], item_label, "input_tokens"
        )
        output_tokens = _validate_non_negative_int(
            item["output_tokens"], item_label, "output_tokens"
        )
        if total_tokens != input_tokens + output_tokens:
            raise ValueError(f"{item_label}: total_tokens must equal input_tokens + output_tokens")
        if "cost_estimate_usd" in item:
            _validate_optional_non_negative_number(
                item["cost_estimate_usd"], item_label, "cost_estimate_usd"
            )

        aggregate_total += total_tokens
        aggregate_input += input_tokens
        aggregate_output += output_tokens

    expected_total, expected_input, expected_output = summary_totals
    if (aggregate_total, aggregate_input, aggregate_output) != summary_totals:
        raise ValueError(
            f"{line_label}: model_breakdown totals must match token_summary "
            f"({expected_total}, {expected_input}, {expected_output})"
        )


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
    if record["framework"] != "mcp-agent":
        raise ValueError(f"{line_label}: framework must be mcp-agent")
    if record["surface"] != "token_summary":
        raise ValueError(f"{line_label}: surface must be token_summary")
    if not isinstance(record["workflow_name"], str) or not record["workflow_name"].strip():
        raise ValueError(f"{line_label}: workflow_name must be a non-empty string")
    if not isinstance(record["run_id"], str) or not record["run_id"].strip():
        raise ValueError(f"{line_label}: run_id must be a non-empty string")
    if not isinstance(record["outcome"], str) or record["outcome"] not in ALLOWED_OUTCOMES:
        allowed = ", ".join(sorted(ALLOWED_OUTCOMES))
        raise ValueError(f"{line_label}: outcome must be one of: {allowed}")

    summary_totals = _validate_token_summary(record["token_summary"], line_label)

    if "cost_estimate_usd" in record:
        _validate_optional_non_negative_number(
            record["cost_estimate_usd"], line_label, "cost_estimate_usd"
        )
    if "tree_ref" in record:
        if not isinstance(record["tree_ref"], str) or not record["tree_ref"].strip():
            raise ValueError(f"{line_label}: tree_ref must be a non-empty string")
    if "model_breakdown" in record:
        _validate_model_breakdown(record["model_breakdown"], summary_totals, line_label)


def _normalized_record(record: dict[str, Any], line_label: str) -> dict[str, Any]:
    normalized = dict(record)
    try:
        normalized["timestamp"] = _parse_rfc3339_utc(str(record["timestamp"]))
    except ValueError as exc:
        raise ValueError(f"{line_label}: {exc}") from exc
    return normalized


def _build_event(
    record: dict[str, Any],
    assay_run_id: str,
    import_time: str,
) -> dict[str, Any]:
    normalized = _normalized_record(record, "artifact")
    data = {
        "external_system": "mcp-agent",
        "external_surface": "token-summary",
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
    assay_run_id = args.assay_run_id or f"import-mcp-agent-{args.input.stem}"

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
