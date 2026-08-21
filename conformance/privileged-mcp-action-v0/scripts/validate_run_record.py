#!/usr/bin/env python3
"""Validate conformance-run shape and cross-field consistency."""

from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path
import re
import sys
from typing import Any
from urllib.parse import urlparse

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from implementations import (  # noqa: E402
    ID_RE,
    ImplementationRegistryError,
    validate_image_reference,
)
from strict_json import MAX_JSON_DEPTH, load_strict_object  # noqa: E402


RUN_SCHEMA = "assay.privileged_mcp_action.conformance_run.v0"
PROFILE = "privileged-mcp-action/v0"
# Captured from the capture document, never reconstructed from PROFILE.
SUITE = "privileged-mcp-action-v0"
FULL_SHA = re.compile(r"^[0-9a-f]{40}$")
# Kept beside the other corpus-shape constants rather than written into the checks, which is how
# "13" survived in seven separate files after the corpus grew: two scripts, this one's own checks,
# a JSON schema, a Rust test, a Python test, two workflows and the profile spec.
EXPECTED_CASE_COUNT = 14
SHA256 = re.compile(r"^sha256:[0-9a-f]{64}$")
STATUSES = {"match", "mismatch", "execution_error", "harness_error"}
CLAIM_NAMES = {
    "policy_decision_recorded",
    "caller_visible_denial",
    "upstream_delivery",
    "external_side_effect",
}
CLAIM_STATUSES = {"confirmed", "incomplete", "refuted"}
SOURCE_CLASSES = {
    "producer_reported",
    "issuer_attested",
    "receiver_receipt",
    "boundary_observed",
    "third_party_observed",
    "unknown",
}
MAX_RUN_RECORD_BYTES = 4 * 1024 * 1024
RUN_NON_CLAIMS = (
    "a matching run demonstrates agreement on the pinned corpus only",
    "a matching run does not establish implementation independence",
    "the scorer records but does not verify the pack's declared source commit",
    "the scorer does not assess security, compliance, or provider outcomes",
    "candidate process limits are operational bounds, not a sandbox",
)
REPRODUCTION_MODES = {
    "blind_from_spec",
    "from_spec_then_conformance",
    "commissioned_clean_room",
    "other_disclosed",
}
IMPLEMENTATION_REQUIRED_KEYS = {
    "name",
    "version",
    "source",
    "commit",
    "reproduction_mode",
}
IMPLEMENTATION_BINDING_KEYS = {"id", "image"}
TOP_LEVEL_KEYS = {
    "schema",
    "profile",
    "suite",
    "source_corpus_digest",
    "rendered_set_digest",
    "pack_sha256",
    "pack_declared_source_commit",
    "pack_provenance_verification",
    "implementation",
    "summary",
    "harness_errors",
    "cases",
    "non_claims",
    "capture_sha256",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def require_run_record_binds_capture(report: dict[str, Any], capture_digest: str) -> None:
    require(bool(SHA256.fullmatch(capture_digest)), "capture digest is malformed")
    require(
        report["capture_sha256"] == capture_digest,
        "capture_sha256 does not address the scored capture bytes",
    )


def load_run_record(path: Path) -> Any:
    """Read one run record through the corpus-wide strict loader.

    The bounds are the same ones every hostile input in this corpus gets, which
    is the point: a second bounded reader here would be a second answer to what
    this project will parse.
    """
    return load_strict_object(
        path,
        label="run record",
        max_bytes=MAX_RUN_RECORD_BYTES,
        max_depth=MAX_JSON_DEPTH,
    )


def validate_normative_surface(value: dict[str, Any]) -> None:
    require(isinstance(value, dict), "observed result must be an object")
    integrity = value.get("bundle_integrity")
    require(integrity in {"pass", "fail"}, "observed bundle_integrity is invalid")
    if integrity == "fail":
        require(
            set(value) == {"bundle_integrity"},
            "integrity-fail observed result has surplus fields",
        )
        return

    verdict = value.get("verdict")
    require(verdict in {"valid", "invalid"}, "pass observed verdict is invalid")
    if verdict == "invalid":
        require(
            set(value) == {"bundle_integrity", "verdict"},
            "invalid observed result has surplus fields",
        )
        return

    require(
        set(value) == {"bundle_integrity", "verdict", "claims"},
        "valid observed result has missing or surplus fields",
    )
    claims = value["claims"]
    require(
        isinstance(claims, dict) and set(claims) == CLAIM_NAMES,
        "valid observed claims are incomplete or contain surplus claims",
    )
    for name, claim in claims.items():
        require(isinstance(claim, dict), f"{name} claim must be an object")
        status = claim.get("status")
        require(status in CLAIM_STATUSES, f"{name} claim status is invalid")
        if status == "incomplete":
            require(
                set(claim) == {"status"},
                f"{name} incomplete claim must omit source_class",
            )
        else:
            require(
                set(claim) == {"status", "source_class"},
                f"{name} decided claim must carry only status and source_class",
            )
            require(
                claim["source_class"] in SOURCE_CLASSES,
                f"{name} claim source_class is invalid",
            )


def validate_run_record(report: dict[str, Any]) -> None:
    require(set(report) == TOP_LEVEL_KEYS, "run record has missing or surplus fields")
    require(report["schema"] == RUN_SCHEMA, "run record schema mismatch")
    require(report["profile"] == PROFILE, "run record profile mismatch")
    require(report["suite"] == SUITE, "run record suite mismatch")
    for field in ("source_corpus_digest", "rendered_set_digest", "pack_sha256", "capture_sha256"):
        require(bool(SHA256.fullmatch(report[field])), f"{field} is malformed")
    require(
        bool(FULL_SHA.fullmatch(report["pack_declared_source_commit"])),
        "pack_declared_source_commit is malformed",
    )
    require(
        report["pack_provenance_verification"] == "not_performed_by_scorer",
        "pack provenance verification value is unsupported",
    )

    implementation = report["implementation"]
    require(isinstance(implementation, dict), "implementation must be an object")
    require(
        set(implementation) >= IMPLEMENTATION_REQUIRED_KEYS
        and set(implementation) <= IMPLEMENTATION_REQUIRED_KEYS | IMPLEMENTATION_BINDING_KEYS,
        "implementation has missing or surplus fields",
    )
    # `id` and `image` are optional so a v0 record produced before this binding
    # existed stays valid, and they travel together so a record cannot name a
    # registry row without naming the image bytes that row addresses.
    binding = set(implementation) & IMPLEMENTATION_BINDING_KEYS
    require(
        binding in (set(), IMPLEMENTATION_BINDING_KEYS),
        "implementation id and image must both be present or both be absent",
    )
    if binding:
        require(
            isinstance(implementation["id"], str)
            and bool(ID_RE.fullmatch(implementation["id"])),
            "implementation id is malformed",
        )
        try:
            validate_image_reference(implementation["image"])
        except ImplementationRegistryError as error:
            raise ValueError("implementation image is invalid: %s" % error) from error
    require(
        isinstance(implementation["name"], str) and bool(implementation["name"]),
        "implementation name is missing",
    )
    require(
        implementation["version"] is None
        or isinstance(implementation["version"], str),
        "implementation version is invalid",
    )
    source = urlparse(implementation.get("source", ""))
    require(
        source.scheme in {"http", "https"} and bool(source.netloc),
        "implementation source must be an absolute HTTP(S) URL",
    )
    require(
        bool(FULL_SHA.fullmatch(implementation.get("commit", ""))),
        "implementation commit is malformed",
    )
    require(
        implementation["reproduction_mode"] in REPRODUCTION_MODES,
        "implementation reproduction_mode is invalid",
    )

    cases = report["cases"]
    require(
        isinstance(cases, list) and len(cases) == EXPECTED_CASE_COUNT,
        f"run must contain {EXPECTED_CASE_COUNT} cases",
    )
    require(all(isinstance(case, dict) for case in cases), "each case must be an object")
    expected_ids = [f"case-{index:03d}" for index in range(1, EXPECTED_CASE_COUNT + 1)]
    require(
        [case.get("case_id") for case in cases] == expected_ids,
        "case ids must be complete and ordered",
    )
    status_counts: Counter[str] = Counter()
    for case in cases:
        status = case.get("status")
        require(status in STATUSES, "case status is invalid")
        require(bool(SHA256.fullmatch(case.get("input_sha256", ""))), "case digest is malformed")
        status_counts[status] += 1
        if status in {"match", "mismatch"}:
            require(
                set(case)
                <= {
                    "case_id",
                    "input_sha256",
                    "status",
                    "observed",
                    "exit_code",
                    "stderr_present",
                    "review_warnings",
                },
                f"{status} case has surplus fields",
            )
            require("observed" in case, f"{status} case is missing observed result")
            validate_normative_surface(case["observed"])
            require(type(case.get("exit_code")) is int, f"{status} case is missing exit_code")
            require(
                isinstance(case.get("stderr_present"), bool),
                f"{status} case is missing stderr_present",
            )
            require("error" not in case, f"{status} case must not carry error")
            warnings = case.get("review_warnings", [])
            require(
                isinstance(warnings, list)
                and all(isinstance(warning, str) for warning in warnings),
                f"{status} review_warnings is invalid",
            )
        else:
            require(
                set(case) == {"case_id", "input_sha256", "status", "error"},
                f"{status} case has missing or surplus fields",
            )
            require(
                isinstance(case.get("error"), str) and bool(case["error"]),
                f"{status} case is missing error",
            )
            require("observed" not in case, f"{status} case must not carry observed result")

    summary = report["summary"]
    require(isinstance(summary, dict), "summary must be an object")
    expected_summary_keys = {
        "total",
        "match",
        "mismatch",
        "execution_error",
        "harness_error",
        "review_warnings",
    }
    require(set(summary) == expected_summary_keys, "summary has missing or surplus fields")
    require(
        all(type(summary[field]) is int and summary[field] >= 0 for field in expected_summary_keys),
        "summary counts must be non-negative integers",
    )
    require(summary["total"] == len(cases), "summary total does not match cases")
    for status in STATUSES:
        require(
            summary[status] == status_counts[status],
            f"summary {status} count does not match cases",
        )
    require(
        sum(status_counts.values()) == summary["total"],
        "case status counts do not match total",
    )
    harness_errors = report["harness_errors"]
    require(
        isinstance(harness_errors, list)
        and all(isinstance(error, str) and bool(error) for error in harness_errors),
        "harness_errors must contain non-empty strings",
    )
    warning_count = sum(len(case.get("review_warnings", [])) for case in cases)
    require(
        summary["review_warnings"] == warning_count,
        "summary review_warnings count does not match cases",
    )
    require(
        report["non_claims"] == list(RUN_NON_CLAIMS),
        "non_claims must match the fixed run-record claim ceiling",
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        report = load_run_record(args.report)
        if not isinstance(report, dict):
            raise ValueError("run record must be an object")
        validate_run_record(report)
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        RecursionError,
        json.JSONDecodeError,
    ) as error:
        print(f"invalid conformance run record: {error}", file=__import__("sys").stderr)
        return 2
    print(f"validated {args.report}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
