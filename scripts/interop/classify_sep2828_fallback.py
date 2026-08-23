#!/usr/bin/env python3
"""Classify Assay's measured SEP-2828 fallback result without hiding drift."""

import json
import sys


EXPECTED_FALSE = {
    "decision_request_envelope_digest_match",
    "outcome_request_envelope_digest_match",
}
REQUIRED_TRUE = {
    "fallback_projection_binding_present",
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
    value = json.load(sys.stdin)
    if not isinstance(value, dict):
        raise ValueError("stdin must contain a JSON object")
    return value


def classify(report: dict, upstream_projection: object) -> dict:
    binding = report.get("binding")
    checks_raw = report.get("checks")
    assay_projection = binding.get("projection") if isinstance(binding, dict) else None

    checks = {}
    if isinstance(checks_raw, list):
        for check in checks_raw:
            if isinstance(check, dict) and isinstance(check.get("id"), str):
                checks[check["id"]] = check.get("ok")
    false_checks = sorted(check_id for check_id, ok in checks.items() if ok is False)

    if report.get("ok") is True:
        disposition = "reproduced"
    else:
        exact_binding = isinstance(binding, dict) and all(
            binding.get(key) == value for key, value in EXPECTED_BINDING.items()
        )
        exact_mismatch = (
            exact_binding
            and isinstance(upstream_projection, str)
            and upstream_projection
            and upstream_projection != assay_projection
            and set(false_checks) == EXPECTED_FALSE
            and all(checks.get(check_id) is True for check_id in REQUIRED_TRUE)
        )
        disposition = "documented_non_reproduction" if exact_mismatch else "diverged"

    return {
        "classification": disposition,
        "assay_projection": assay_projection,
        "upstream_projection": upstream_projection,
        "false_checks": false_checks,
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
