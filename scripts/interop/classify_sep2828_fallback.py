#!/usr/bin/env python3
"""Classify Assay's measured SEP-2828 fallback result without hiding drift."""

import json
import sys


EXPECTED_FALSE = {
    "fallback_projection_missing_params",
}
FORBIDDEN_UNRESOLVED_CHECKS = {
    "fallback_projection_binding_present",
    "decision_request_envelope_digest_match",
    "outcome_request_envelope_digest_match",
}
EXPECTED_UPSTREAM_PROJECTION = "tools_call_params_plus_meta_authorization_binding_v1"
REQUIRED_NONCLAIM = "fallback_call_parameter_binding"
MAX_REPORT_BYTES = 1024 * 1024
REQUIRED_TRUE = {
    "decision_request_envelope_nonce_present",
    "decision_outcome_backlink_match",
    "outcome_decision_digest_match",
    "result_commitment_projection_digest_match",
}
EXPECTED_BINDING = {
    "mode": "request_envelope",
    "projection": "assay.fallback_projection.v0",
    "digest_source": "request_envelope_named_projection_jcs",
}


def load_report() -> dict:
    raw = sys.stdin.buffer.read(MAX_REPORT_BYTES + 1)
    if len(raw) > MAX_REPORT_BYTES:
        raise ValueError(f"stdin exceeds {MAX_REPORT_BYTES} bytes")
    value = json.loads(raw)
    if not isinstance(value, dict):
        raise ValueError("stdin must contain a JSON object")
    return value


def classify(report: dict, upstream_projection: object) -> dict:
    binding = report.get("binding")
    checks_raw = report.get("checks")
    claims_not_made = report.get("claims_not_made")
    assay_projection = binding.get("projection") if isinstance(binding, dict) else None

    checks = {}
    duplicate_check_ids = set()
    malformed_checks = not isinstance(checks_raw, list)
    if isinstance(checks_raw, list):
        for check in checks_raw:
            if not (
                isinstance(check, dict)
                and isinstance(check.get("id"), str)
                and isinstance(check.get("ok"), bool)
            ):
                malformed_checks = True
                continue
            if check["id"] in checks:
                duplicate_check_ids.add(check["id"])
            checks[check["id"]] = check["ok"]
    false_checks = sorted(check_id for check_id, ok in checks.items() if ok is False)

    if report.get("ok") is True:
        disposition = "reproduced"
    else:
        exact_binding = isinstance(binding, dict) and all(
            binding.get(key) == value for key, value in EXPECTED_BINDING.items()
        )
        unresolved_digest = (
            isinstance(binding, dict)
            and "digest" in binding
            and binding["digest"] is None
        )
        exact_mismatch = (
            report.get("ok") is False
            and exact_binding
            and unresolved_digest
            and upstream_projection == EXPECTED_UPSTREAM_PROJECTION
            and upstream_projection != assay_projection
            and not malformed_checks
            and not duplicate_check_ids
            and set(false_checks) == EXPECTED_FALSE
            and all(checks.get(check_id) is True for check_id in REQUIRED_TRUE)
            and FORBIDDEN_UNRESOLVED_CHECKS.isdisjoint(checks)
            and isinstance(claims_not_made, list)
            and REQUIRED_NONCLAIM in claims_not_made
        )
        disposition = "documented_non_reproduction" if exact_mismatch else "diverged"

    return {
        "classification": disposition,
        "assay_projection": assay_projection,
        "upstream_projection": upstream_projection,
        "false_checks": false_checks,
        "duplicate_check_ids": sorted(duplicate_check_ids),
    }


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: classify_sep2828_fallback.py UPSTREAM_PROJECTION_JSON < REPORT", file=sys.stderr)
        return 2
    try:
        upstream_projection = json.loads(sys.argv[1])
        result = classify(load_report(), upstream_projection)
    except (ValueError, json.JSONDecodeError) as error:
        print(f"failed to classify fallback result: {error}", file=sys.stderr)
        return 2
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
