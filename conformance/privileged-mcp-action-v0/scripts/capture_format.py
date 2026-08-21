#!/usr/bin/env python3
"""The candidate-capture artifact: what an untrusted host may record, and nothing else.

A capture holds observations of a candidate's own output plus the bindings a
trusted scorer needs to refuse a capture that does not belong to the pack it was
told to score. It carries no expected value, no match or mismatch, no count of
agreement, no score and no badge, because the host that writes it is the host
hostile code runs on.

Observation and judgement are split. The capture host records three bounded facts
about the candidate's own report; the trusted side owns the rule that turns those
facts into reviewer warnings. That also keeps unbounded free-form findings out of
the artifact entirely.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from artifact_io import content_sha256  # noqa: E402
from strict_json import parse_strict_object  # noqa: E402
from validate_run_record import (  # noqa: E402
    EXPECTED_CASE_COUNT,
    FULL_SHA,
    PROFILE,
    SHA256,
    validate_normative_surface,
)

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from implementations import (  # noqa: E402
    ID_RE,
    ImplementationRegistryError,
    validate_image_reference,
)

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "adequacy"))
import published_rows  # noqa: E402

CAPTURE_SCHEMA = "assay.privileged_mcp_action.candidate_capture.v0"
REPORT_SCHEMA = "assay.privileged_mcp_action.verify.report.v0"
SUITE = "privileged-mcp-action-v0"

# Fourteen bounded observations plus a fixed header. The ceiling is two orders
# of magnitude of slack over the largest honest capture, and it exists so a
# capture cannot be used to hand the trusted host an arbitrary payload.
MAX_CAPTURE_BYTES = 64 * 1024
MAX_CAPTURE_DEPTH = 8
# Error text is candidate-influenced. It is bounded here rather than trusted to
# stay short, because "the messages we emit today are short" is a property of
# today's messages and not of the format.
MAX_ERROR_CHARS = 512
# POSIX exit statuses, plus the negative form subprocess reports for a signal.
MIN_EXIT_CODE = -256
MAX_EXIT_CODE = 255

STATE_OBSERVED = "observed"
STATE_CANDIDATE_ERROR = "candidate_error"
STATE_CAPTURE_ERROR = "capture_error"
OBSERVATION_STATES = (STATE_OBSERVED, STATE_CANDIDATE_ERROR, STATE_CAPTURE_ERROR)

REPRODUCTION_MODES = (
    "blind_from_spec",
    "from_spec_then_conformance",
    "commissioned_clean_room",
    "other_disclosed",
)

CAPTURE_NON_CLAIMS = (
    "a capture records what a candidate emitted; it is not a verdict",
    "a capture host can fabricate observations, so a capture is not authenticated",
    "candidate process limits are operational bounds, not a sandbox",
    "a declared image digest addresses bytes; it does not prove which image ran",
)

TOP_LEVEL_KEYS = {
    "schema",
    "profile",
    "suite",
    "pack_sha256",
    "pack_declared_source_commit",
    "source_corpus_digest",
    "rendered_set_digest",
    "implementation",
    "observations",
    "capture_non_claims",
}
IMPLEMENTATION_KEYS = {
    "id",
    "image",
    "name",
    "version",
    "source",
    "commit",
    "reproduction_mode",
}
OBSERVED_KEYS = {
    "case_id",
    "input_sha256",
    "state",
    "observed",
    "exit_code",
    "stderr_present",
    "reviewer_reason_present",
    "report_schema_matches",
    "report_profile_matches",
}
ERROR_KEYS = {"case_id", "input_sha256", "state", "error"}

REJECT_REASON_MISSING = "reject_reason_missing"
REPORT_SCHEMA_WARNING = "report_schema_missing_or_unexpected"
REPORT_PROFILE_WARNING = "report_profile_missing_or_unexpected"


class CaptureError(ValueError):
    """A capture does not bind. Never turned into a partial result."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CaptureError(message)


def normative_surface(report: dict[str, Any]) -> dict[str, Any]:
    """The only fields a run compares. One definition, used by both phases."""
    result = {"bundle_integrity": report["bundle_integrity"]}
    if report["bundle_integrity"] == "pass":
        result["verdict"] = report["verdict"]
        if report["verdict"] == "valid":
            result["claims"] = report["claims"]
    return result


def has_reviewer_reason(report: dict[str, Any]) -> bool:
    findings = report.get("findings")
    if not isinstance(findings, list):
        return False
    return any(
        (isinstance(finding, str) and bool(finding.strip()))
        or (
            isinstance(finding, dict)
            and any(
                isinstance(value, str) and bool(value.strip())
                for value in finding.values()
            )
        )
        for finding in findings
    )


def bound_error(message: str) -> str:
    text = str(message).strip() or "unspecified error"
    if len(text) > MAX_ERROR_CHARS:
        text = text[: MAX_ERROR_CHARS - 1] + "…"
    return text


def observe(
    case_id: str,
    input_sha256: str,
    report: dict[str, Any],
    exit_code: int,
    stderr_present: bool,
) -> dict[str, Any]:
    """One observation of one candidate execution. Records facts, decides nothing."""
    return {
        "case_id": case_id,
        "input_sha256": input_sha256,
        "state": STATE_OBSERVED,
        "observed": normative_surface(report),
        "exit_code": exit_code,
        "stderr_present": bool(stderr_present),
        "reviewer_reason_present": has_reviewer_reason(report),
        "report_schema_matches": report.get("schema") == REPORT_SCHEMA,
        "report_profile_matches": report.get("profile") == PROFILE,
    }


def observe_error(case_id: str, input_sha256: str, state: str, message: str) -> dict[str, Any]:
    return {
        "case_id": case_id,
        "input_sha256": input_sha256,
        "state": state,
        "error": bound_error(message),
    }


def review_warnings(observation: dict[str, Any]) -> list[str]:
    """The reviewer-warning rule. Owned by the trusted side, from recorded facts."""
    if observation["state"] != STATE_OBSERVED:
        return []
    observed = observation["observed"]
    warnings = []
    if (
        observed.get("bundle_integrity") == "pass"
        and observed.get("verdict") == "invalid"
        and not observation["reviewer_reason_present"]
    ):
        warnings.append(REJECT_REASON_MISSING)
    if not observation["report_schema_matches"]:
        warnings.append(REPORT_SCHEMA_WARNING)
    if not observation["report_profile_matches"]:
        warnings.append(REPORT_PROFILE_WARNING)
    return warnings


def expected_case_ids() -> list[str]:
    return [f"case-{index:03d}" for index in range(1, EXPECTED_CASE_COUNT + 1)]


def validate_implementation(implementation: Any) -> None:
    require(isinstance(implementation, dict), "capture implementation must be an object")
    require(
        set(implementation) == IMPLEMENTATION_KEYS,
        "capture implementation has missing or surplus fields",
    )
    require(
        isinstance(implementation["name"], str) and bool(implementation["name"]),
        "capture implementation name is missing",
    )
    require(
        implementation["version"] is None or isinstance(implementation["version"], str),
        "capture implementation version is invalid",
    )
    source = implementation["source"]
    require(
        isinstance(source, str) and source.startswith(("http://", "https://")),
        "capture implementation source must be an absolute HTTP(S) URL",
    )
    require(
        isinstance(implementation["commit"], str)
        and bool(FULL_SHA.fullmatch(implementation["commit"])),
        "capture implementation commit is malformed",
    )
    require(
        implementation["reproduction_mode"] in REPRODUCTION_MODES,
        "capture implementation reproduction_mode is invalid",
    )
    identifier = implementation["id"]
    image = implementation["image"]
    # Either the capture names a registered image or it declares that it names
    # none. A half-binding would read as a row reference the run record cannot
    # honour, so the two travel together or not at all.
    require(
        (identifier is None) == (image is None),
        "capture implementation id and image must both be present or both be null",
    )
    if identifier is None:
        return
    require(
        isinstance(identifier, str) and bool(ID_RE.fullmatch(identifier)),
        "capture implementation id is malformed",
    )
    try:
        validate_image_reference(image)
    except ImplementationRegistryError as error:
        raise CaptureError(f"capture implementation image is invalid: {error}") from error


def validate_observation(observation: Any, index: int, case_id: str) -> None:
    require(isinstance(observation, dict), f"observation {index} must be an object")
    require(
        observation.get("case_id") == case_id,
        "capture case ids must be complete and ordered",
    )
    require(
        isinstance(observation.get("input_sha256"), str)
        and bool(SHA256.fullmatch(observation["input_sha256"])),
        f"{case_id}: capture input digest is malformed",
    )
    state = observation.get("state")
    require(state in OBSERVATION_STATES, f"{case_id}: capture state is invalid")
    if state != STATE_OBSERVED:
        require(set(observation) == ERROR_KEYS, f"{case_id}: error observation has missing or surplus fields")
        error = observation["error"]
        require(
            isinstance(error, str) and bool(error) and len(error) <= MAX_ERROR_CHARS,
            f"{case_id}: capture error text is missing or exceeds {MAX_ERROR_CHARS} characters",
        )
        return
    require(set(observation) == OBSERVED_KEYS, f"{case_id}: observation has missing or surplus fields")
    try:
        validate_normative_surface(observation["observed"])
    except (KeyError, TypeError, ValueError) as error:
        raise CaptureError(f"{case_id}: observed result is invalid: {error}") from error
    exit_code = observation["exit_code"]
    require(type(exit_code) is int, f"{case_id}: capture exit_code must be an int")
    require(
        MIN_EXIT_CODE <= exit_code <= MAX_EXIT_CODE,
        f"{case_id}: capture exit_code is outside [{MIN_EXIT_CODE}, {MAX_EXIT_CODE}]",
    )
    for field in ("stderr_present", "reviewer_reason_present", "report_schema_matches",
                  "report_profile_matches"):
        require(
            isinstance(observation[field], bool),
            f"{case_id}: capture {field} must be a boolean",
        )


def validate_capture(capture: Any) -> None:
    """Whole-capture validation. All or nothing, before any comparison happens."""
    require(isinstance(capture, dict), "capture must be an object")
    require(set(capture) == TOP_LEVEL_KEYS, "capture has missing or surplus fields")
    require(capture["schema"] == CAPTURE_SCHEMA, "capture schema mismatch")
    require(capture["profile"] == PROFILE, "capture profile mismatch")
    require(capture["suite"] == SUITE, "capture suite mismatch")
    for field in ("pack_sha256", "source_corpus_digest", "rendered_set_digest"):
        require(
            isinstance(capture[field], str) and bool(SHA256.fullmatch(capture[field])),
            f"capture {field} is malformed",
        )
    require(
        isinstance(capture["pack_declared_source_commit"], str)
        and bool(FULL_SHA.fullmatch(capture["pack_declared_source_commit"])),
        "capture pack_declared_source_commit is malformed",
    )
    validate_implementation(capture["implementation"])
    observations = capture["observations"]
    require(
        isinstance(observations, list) and len(observations) == EXPECTED_CASE_COUNT,
        f"capture must contain {EXPECTED_CASE_COUNT} observations",
    )
    for index, (observation, case_id) in enumerate(zip(observations, expected_case_ids())):
        validate_observation(observation, index, case_id)
    require(
        capture["capture_non_claims"] == list(CAPTURE_NON_CLAIMS),
        "capture_non_claims must match the fixed capture claim ceiling",
    )


def load_capture_with_digest(path: Path) -> tuple[dict, str]:
    """Read hostile capture bytes once, bind their digest, then validate them whole."""
    try:
        data = published_rows.read_regular_file(Path(path), limit=MAX_CAPTURE_BYTES)
        document = parse_strict_object(
            data,
            label="capture",
            max_depth=MAX_CAPTURE_DEPTH,
        )
    except ValueError as error:
        raise CaptureError(str(error)) from error
    validate_capture(document)
    return document, content_sha256(data)


def load_capture(path: Path) -> dict:
    """Read one capture as hostile input, then validate it whole."""
    return load_capture_with_digest(path)[0]


def add_identity_arguments(parser: "argparse.ArgumentParser", *, required: bool = True) -> None:
    """One place that states what identifies an implementation to this corpus."""
    parser.add_argument("--implementation-name", required=required)
    parser.add_argument("--implementation-version")
    parser.add_argument("--implementation-source", required=required)
    parser.add_argument("--implementation-commit", required=required)
    parser.add_argument(
        "--reproduction-mode", choices=REPRODUCTION_MODES, required=required
    )
    parser.add_argument(
        "--implementation-id",
        help="registry row id from conformance/implementations.json",
    )
    parser.add_argument(
        "--implementation-image",
        help="exact name@sha256:<64 hex> of the image that ran, if one did",
    )


def identity_from_args(args: argparse.Namespace) -> dict[str, Any]:
    return {
        "id": args.implementation_id,
        "image": args.implementation_image,
        "name": args.implementation_name,
        "version": args.implementation_version,
        "source": args.implementation_source,
        "commit": args.implementation_commit,
        "reproduction_mode": args.reproduction_mode,
    }
