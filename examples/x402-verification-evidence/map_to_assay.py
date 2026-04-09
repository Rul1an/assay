"""Map a frozen x402 requirement/verification artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any, Optional


PLACEHOLDER_EVENT_TYPE = "example.placeholder.x402-requirement-verification"
PLACEHOLDER_SOURCE = "urn:example:assay:external:x402:requirement-verification"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "x402.requirement-verification.export.v1"
REQUIRED_KEYS = (
    "schema",
    "protocol",
    "surface",
    "transport",
    "x402_version",
    "resource_ref",
    "scheme",
    "network",
    "amount",
    "asset_ref",
    "timestamp",
    "verification_result",
)
OPTIONAL_KEYS = {
    "invalid_reason",
    "invalid_message_ref",
    "facilitator_ref",
    "timeout_seconds",
    "payment_identifier_ref",
    "payee_ref",
}
ALLOWED_TRANSPORTS = {"http"}
ALLOWED_VERIFICATION_RESULTS = {"verified", "rejected"}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one x402 requirement/verification artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="x402 artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-x402-<stem>.",
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
        raise ValueError("non-integer floats are not valid in this sample's canonical JSON subset")
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
    return value


def _validate_amount(value: Any, line_label: str) -> str:
    text = _validate_non_empty_string(value, line_label, "amount")
    if not text.isdigit():
        raise ValueError(f"{line_label}: amount must be a string of decimal digits")
    if int(text) <= 0:
        raise ValueError(f"{line_label}: amount must be greater than zero")
    return text


def _validate_optional_ref(value: Any, line_label: str, field_name: str) -> None:
    _validate_non_empty_string(value, line_label, field_name)


def _validate_invalid_reason(value: Any, line_label: str) -> str:
    reason = _validate_non_empty_string(value, line_label, "invalid_reason")
    allowed = set("abcdefghijklmnopqrstuvwxyz0123456789_-")
    if any(char not in allowed for char in reason):
        raise ValueError(
            f"{line_label}: invalid_reason must be a short classifier using lowercase letters, digits, '_' or '-'"
        )
    return reason


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
    if record["protocol"] != "x402":
        raise ValueError(f"{line_label}: protocol must be x402")
    if record["surface"] != "requirement_verify_result":
        raise ValueError(f"{line_label}: surface must be requirement_verify_result")

    transport = _validate_non_empty_string(record["transport"], line_label, "transport")
    if transport not in ALLOWED_TRANSPORTS:
        allowed = ", ".join(sorted(ALLOWED_TRANSPORTS))
        raise ValueError(f"{line_label}: transport must be one of: {allowed}")

    if not isinstance(record["x402_version"], int) or isinstance(record["x402_version"], bool):
        raise ValueError(f"{line_label}: x402_version must be an integer")
    if record["x402_version"] != 2:
        raise ValueError(f"{line_label}: x402_version must be 2")

    for field in ("resource_ref", "scheme", "network", "asset_ref"):
        _validate_non_empty_string(record[field], line_label, field)

    _validate_amount(record["amount"], line_label)

    verification_result = _validate_non_empty_string(
        record["verification_result"], line_label, "verification_result"
    )
    if verification_result not in ALLOWED_VERIFICATION_RESULTS:
        allowed = ", ".join(sorted(ALLOWED_VERIFICATION_RESULTS))
        raise ValueError(f"{line_label}: verification_result must be one of: {allowed}")

    try:
        _parse_rfc3339_utc(str(record["timestamp"]))
    except ValueError as exc:
        raise ValueError(f"{line_label}: {exc}") from exc

    for field in ("invalid_message_ref", "facilitator_ref", "payment_identifier_ref", "payee_ref"):
        if field in record:
            _validate_optional_ref(record[field], line_label, field)

    if "invalid_reason" in record:
        _validate_invalid_reason(record["invalid_reason"], line_label)

    if "timeout_seconds" in record:
        value = record["timeout_seconds"]
        if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
            raise ValueError(f"{line_label}: timeout_seconds must be a positive integer")


def _normalized_record(record: dict[str, Any]) -> dict[str, Any]:
    normalized = dict(record)
    normalized["timestamp"] = _parse_rfc3339_utc(str(record["timestamp"]))
    return normalized


def _build_event(record: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    normalized = _normalized_record(record)
    data = {
        "external_system": "x402",
        "external_surface": "requirement-verification-result",
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
        _validate_record(record, "artifact")
        import_time = _parse_rfc3339_utc(args.import_time)
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc

    assay_run_id = args.assay_run_id or f"import-x402-{args.input.stem}"
    event = _build_event(record, assay_run_id, import_time)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    try:
        args.output.write_text(_canonical_json(event) + "\n", encoding="utf-8")
    except OSError as exc:
        raise SystemExit(str(exc)) from exc
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
