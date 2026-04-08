"""Map a frozen Visa TAP verification artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
from typing import Any


PLACEHOLDER_EVENT_TYPE = "example.placeholder.tap-verification"
PLACEHOLDER_SOURCE = "urn:example:assay:external:tap:signature-verification"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "tap.signature_verification.export.v1"
REQUIRED_KEYS = (
    "schema",
    "protocol",
    "surface",
    "timestamp",
    "session_id",
    "key_id",
    "algorithm",
    "merchant_domain_ref",
    "operation_type",
    "verification_result",
)
OPTIONAL_KEYS = {
    "agent_ref",
    "registry_ref",
    "verification_reason",
    "request_ref",
}
ALLOWED_VERIFICATION_RESULTS = {"verified", "rejected"}
ALLOWED_OPERATION_TYPES = {"browsing", "payment"}
ALLOWED_VERIFICATION_REASONS = {
    "signature_mismatch",
    "domain_mismatch",
    "operation_mismatch",
    "replay_detected",
    "unknown_key",
    "expired",
}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one Visa TAP verification artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="TAP verification artifact to read.")
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
        help="Optional Assay run id override. Defaults to import-tap-<stem>.",
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
    if isinstance(value, dict):
        return {str(key): _normalize_for_hash(nested) for key, nested in value.items()}
    if isinstance(value, list):
        return [_normalize_for_hash(item) for item in value]
    if isinstance(value, tuple):
        return [_normalize_for_hash(item) for item in value]
    raise TypeError(f"unsupported canonical JSON value: {type(value).__name__}")


def _canonical_json(value: Any) -> str:
    # This sample emits deterministic sorted-key JSON for the validated fixture
    # corpus, but it is not a full RFC 8785 / JCS implementation for arbitrary
    # JSON inputs or cross-implementation hashing.
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


def _validate_non_empty_string(value: Any, line_label: str, field_name: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{line_label}: {field_name} must be a non-empty string")
    return value


def _validate_domain_ref(value: Any, line_label: str, field_name: str) -> str:
    domain = _validate_non_empty_string(value, line_label, field_name)
    if domain != domain.strip().lower():
        raise ValueError(f"{line_label}: {field_name} must be lowercase and normalized")
    if " " in domain or "/" in domain or ":" in domain:
        raise ValueError(f"{line_label}: {field_name} must be a normalized hostname only")
    if "." not in domain:
        raise ValueError(f"{line_label}: {field_name} must look like a hostname")
    return domain


def _validate_optional_ref(value: Any, line_label: str, field_name: str) -> None:
    ref = _validate_non_empty_string(value, line_label, field_name)
    if field_name == "agent_ref":
        lowered = ref.lower()
        if "@" in ref or lowered.startswith("mailto:") or "user" in lowered:
            raise ValueError(
                f"{line_label}: agent_ref must be opaque and must not imply a user identity"
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
    if record["protocol"] != "tap":
        raise ValueError(f"{line_label}: protocol must be tap")
    if record["surface"] != "signature_verification":
        raise ValueError(f"{line_label}: surface must be signature_verification")

    _validate_non_empty_string(record["session_id"], line_label, "session_id")
    _validate_non_empty_string(record["key_id"], line_label, "key_id")
    _validate_non_empty_string(record["algorithm"], line_label, "algorithm")
    _validate_domain_ref(record["merchant_domain_ref"], line_label, "merchant_domain_ref")

    if not isinstance(record["operation_type"], str) or record["operation_type"] not in ALLOWED_OPERATION_TYPES:
        allowed = ", ".join(sorted(ALLOWED_OPERATION_TYPES))
        raise ValueError(f"{line_label}: operation_type must be one of: {allowed}")

    if (
        not isinstance(record["verification_result"], str)
        or record["verification_result"] not in ALLOWED_VERIFICATION_RESULTS
    ):
        allowed = ", ".join(sorted(ALLOWED_VERIFICATION_RESULTS))
        raise ValueError(f"{line_label}: verification_result must be one of: {allowed}")

    if "verification_reason" in record:
        reason = _validate_non_empty_string(record["verification_reason"], line_label, "verification_reason")
        if reason not in ALLOWED_VERIFICATION_REASONS:
            allowed = ", ".join(sorted(ALLOWED_VERIFICATION_REASONS))
            raise ValueError(f"{line_label}: verification_reason must be one of: {allowed}")

    if record["verification_result"] == "verified" and "verification_reason" in record:
        raise ValueError(
            f"{line_label}: verified artifacts must not carry verification_reason"
        )
    if record["verification_result"] == "rejected" and "verification_reason" not in record:
        raise ValueError(
            f"{line_label}: rejected artifacts must carry verification_reason"
        )

    for field in ("agent_ref", "registry_ref", "request_ref"):
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
        "external_system": "tap",
        "external_surface": "signature-verification",
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
        with args.input.open("r", encoding="utf-8") as handle:
            record = json.load(handle, object_pairs_hook=_reject_duplicate_keys)
    except (json.JSONDecodeError, ValueError) as exc:
        raise SystemExit(str(exc)) from exc

    if not isinstance(record, dict):
        raise SystemExit("artifact: top-level JSON value must be an object")

    try:
        _validate_record(record, "artifact")
        assay_run_id = args.assay_run_id or f"import-tap-{args.input.stem}"
        import_time = _parse_rfc3339_utc(args.import_time)
        event = _build_event(record, assay_run_id, import_time)
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", encoding="utf-8") as handle:
        handle.write(_canonical_json(event))
        handle.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
