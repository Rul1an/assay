#!/usr/bin/env python3
"""Score a candidate's observations against the canonical oracle. Trusted phase.

Two ways in, one comparison. `--capture` scores a capture written elsewhere by
`capture_candidate.py`, so the host holding the oracle never executes candidate
code. `--entrypoint` still captures and scores in one process, and it does so by
calling the same capture builder and the same scorer, which is why both produce
byte-identical run records for the same candidate.

Pack, corpus and case bindings are recomputed here from this host's own copy of
the pack. The capture's copies of those values are compared against the
recomputed ones and never adopted; a capture that does not bind is refused
before any comparison, and no run record is written.
"""

from __future__ import annotations

import argparse
import json
import shlex
import sys
import tarfile
import tempfile
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

sys.path.insert(0, str(Path(__file__).resolve().parent))
from capture_candidate import (  # noqa: E402
    build_capture,
    capture_observations,
    load_pack_with_digest,
    positive_int,
    sha256,
)
from artifact_io import (  # noqa: E402
    render_deterministic_json_bytes,
    write_regular_file_atomically,
)
from capture_format import (  # noqa: E402
    STATE_CANDIDATE_ERROR,
    STATE_CAPTURE_ERROR,
    STATE_OBSERVED,
    CaptureError,
    add_identity_arguments,
    identity_from_args,
    load_capture,
    normative_surface,
    review_warnings,
    validate_capture,
)
from pack_format import ordered_vectors, rewrite_bundle_stream_identity  # noqa: E402
from validate_run_record import (  # noqa: E402
    FULL_SHA,
    PROFILE,
    RUN_NON_CLAIMS,
    RUN_SCHEMA,
    validate_run_record,
)

# A capture state names what happened on the capture host; a run-record status
# names what the run concluded. The two vocabularies are joined in one place.
STATE_TO_STATUS = {
    STATE_CANDIDATE_ERROR: "execution_error",
    STATE_CAPTURE_ERROR: "harness_error",
}


def load_expectations(manifest_path: Path) -> tuple[dict[str, Any], str, str]:
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    expected_by_hash: dict[str, Any] = {}
    vectors = ordered_vectors(manifest["vectors"])
    for index, vector in enumerate(vectors, start=1):
        source = (manifest_path.parent / vector["file"]).read_bytes()
        if sha256(source) != vector["sha256"]:
            raise ValueError(f"canonical source digest mismatch for {vector['file']}")
        transformed = rewrite_bundle_stream_identity(source, f"pmav0-case-{index:03d}")
        digest = sha256(transformed)
        if digest in expected_by_hash:
            raise ValueError(f"duplicate rendered vector digest: {digest}")
        expected = vector["expected"]
        projected = normative_surface(expected)
        if projected != expected:
            raise ValueError(
                f"canonical expected surface contains surplus fields for {vector['file']}"
            )
        expected_by_hash[digest] = projected
    rendered_set_digest = sha256(
        "".join(digest + "\n" for digest in expected_by_hash).encode()
    )
    return expected_by_hash, manifest["corpus_digest"], rendered_set_digest


def require_capture_binds_pack(capture: dict[str, Any], pack: dict[str, Any], pack_digest: str) -> None:
    """Compare the capture's bindings against locally recomputed values.

    Fatal, and fatal before comparison. A capture that names a different pack has
    not observed the cases this host is about to score, and a partial run record
    would publish agreement it did not measure.
    """
    for field, recomputed in (
        ("pack_sha256", pack_digest),
        ("pack_declared_source_commit", pack["declared_source_commit"]),
        ("source_corpus_digest", pack["source_corpus_digest"]),
        ("rendered_set_digest", pack["rendered_set_digest"]),
    ):
        if capture[field] != recomputed:
            raise CaptureError(
                f"capture {field} does not bind this pack"
            )
    for observation, case in zip(capture["observations"], pack["cases"]):
        if observation["case_id"] != case["id"]:
            raise CaptureError("capture case ids do not bind this pack in order")
        if observation["input_sha256"] != case["sha256"]:
            raise CaptureError(
                f"{case['id']}: capture input digest does not bind this pack"
            )


def implementation_record(declared: dict[str, Any]) -> dict[str, Any]:
    """Carry the capture's declared identity. Shape-validated, never verified.

    `id` and `image` appear only when the capture declared them, so an existing
    v0 run record with neither stays valid and a run that named no image does
    not grow a field implying one.
    """
    record = {
        "name": declared["name"],
        "version": declared["version"],
        "source": declared["source"],
        "commit": declared["commit"],
        "reproduction_mode": declared["reproduction_mode"],
    }
    if declared["id"] is not None:
        record["id"] = declared["id"]
        record["image"] = declared["image"]
    return record


def score_capture(
    capture: dict[str, Any],
    pack: dict[str, Any],
    pack_digest: str,
    expected_by_hash: dict[str, Any],
    source_corpus_digest: str,
    rendered_set_digest: str,
) -> dict[str, Any]:
    """Compare recorded observations with the canonical oracle. The only oracle."""
    harness_errors = []
    if source_corpus_digest != pack["source_corpus_digest"]:
        harness_errors.append("pack and canonical source corpus digests differ")
    if rendered_set_digest != pack["rendered_set_digest"]:
        harness_errors.append("pack and canonical rendered-set digests differ")
    pack_hashes = {case["sha256"] for case in pack["cases"]}
    expectation_hashes = set(expected_by_hash)
    missing_expectations = pack_hashes - expectation_hashes
    missing_cases = expectation_hashes - pack_hashes
    if missing_expectations:
        harness_errors.append(
            f"{len(missing_expectations)} opaque case(s) lack canonical expectations"
        )
    if missing_cases:
        harness_errors.append(
            f"{len(missing_cases)} canonical expectation(s) lack opaque cases"
        )

    cases = []
    counts = {
        "total": len(capture["observations"]),
        "match": 0,
        "mismatch": 0,
        "execution_error": 0,
        "harness_error": 0,
        "review_warnings": 0,
    }
    for observation in capture["observations"]:
        result: dict[str, Any] = {
            "case_id": observation["case_id"],
            "input_sha256": observation["input_sha256"],
        }
        if observation["state"] != STATE_OBSERVED:
            status = STATE_TO_STATUS[observation["state"]]
            result.update(status=status, error=observation["error"])
            counts[status] += 1
            cases.append(result)
            continue
        observed = observation["observed"]
        expected = expected_by_hash.get(observation["input_sha256"])
        if expected is None:
            result.update(
                status="harness_error",
                error="opaque case is absent from canonical expectations",
            )
            counts["harness_error"] += 1
            cases.append(result)
            continue
        result.update(
            status="match" if observed == expected else "mismatch",
            observed=observed,
            exit_code=observation["exit_code"],
            stderr_present=observation["stderr_present"],
        )
        counts[result["status"]] += 1
        warnings = review_warnings(observation)
        if warnings:
            result["review_warnings"] = warnings
            counts["review_warnings"] += len(warnings)
        cases.append(result)

    return {
        "schema": RUN_SCHEMA,
        "profile": PROFILE,
        "source_corpus_digest": pack["source_corpus_digest"],
        "rendered_set_digest": pack["rendered_set_digest"],
        "pack_sha256": pack_digest,
        "pack_declared_source_commit": pack["declared_source_commit"],
        "pack_provenance_verification": "not_performed_by_scorer",
        "implementation": implementation_record(capture["implementation"]),
        "summary": counts,
        "harness_errors": harness_errors,
        "cases": cases,
        "non_claims": list(RUN_NON_CLAIMS),
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--pack", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--entrypoint")
    source.add_argument(
        "--capture",
        type=Path,
        help="score a capture recorded by capture_candidate.py; runs no candidate code",
    )
    add_identity_arguments(parser, required=False)
    parser.add_argument("--timeout-seconds", type=positive_int, default=30)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def check_entrypoint_identity(args: argparse.Namespace) -> str | None:
    for flag, value in (
        ("--implementation-name", args.implementation_name),
        ("--implementation-source", args.implementation_source),
        ("--implementation-commit", args.implementation_commit),
        ("--reproduction-mode", args.reproduction_mode),
    ):
        if value is None:
            return f"{flag} is required with --entrypoint"
    implementation_source = urlparse(args.implementation_source)
    if implementation_source.scheme not in {"http", "https"} or not implementation_source.netloc:
        return "--implementation-source must be an absolute HTTP(S) URL"
    if not FULL_SHA.fullmatch(args.implementation_commit):
        return "--implementation-commit must be a full lowercase 40-hex commit"
    return None


def main() -> int:
    args = parse_args()
    command: list[str] = []
    if args.entrypoint is not None:
        problem = check_entrypoint_identity(args)
        if problem is not None:
            print(problem, file=sys.stderr)
            return 2
        command = shlex.split(args.entrypoint)
        if not command:
            print("--entrypoint must not be empty", file=sys.stderr)
            return 2

    with tempfile.TemporaryDirectory() as tmp:
        try:
            pack, pack_digest = load_pack_with_digest(args.pack, Path(tmp))
        except (
            OSError,
            EOFError,
            ValueError,
            KeyError,
            RecursionError,
            json.JSONDecodeError,
            tarfile.TarError,
        ) as error:
            print(f"invalid clean-room pack: {error}", file=sys.stderr)
            return 2

        try:
            (
                expected_by_hash,
                source_corpus_digest,
                rendered_set_digest,
            ) = load_expectations(args.manifest)
        except (
            OSError,
            EOFError,
            ValueError,
            KeyError,
            RecursionError,
            json.JSONDecodeError,
        ) as error:
            print(f"cannot load canonical expectations: {error}", file=sys.stderr)
            return 2

        try:
            if args.capture is not None:
                capture = load_capture(args.capture)
            else:
                # The oracle is already resident, so this path is for candidates
                # the operator trusts. capture_candidate.py exists for the ones
                # they do not.
                observations = capture_observations(pack, command, args.timeout_seconds)
                capture = build_capture(
                    pack, pack_digest, observations, identity_from_args(args)
                )
                validate_capture(capture)
            require_capture_binds_pack(capture, pack, pack_digest)
        except (CaptureError, OSError) as error:
            print(f"capture does not bind: {error}", file=sys.stderr)
            return 2

    report = score_capture(
        capture,
        pack,
        pack_digest,
        expected_by_hash,
        source_corpus_digest,
        rendered_set_digest,
    )
    try:
        validate_run_record(report)
    except (KeyError, TypeError, ValueError) as error:
        print(f"scorer produced an invalid run record: {error}", file=sys.stderr)
        return 2
    args.output.parent.mkdir(parents=True, exist_ok=True)
    write_regular_file_atomically(args.output, render_deterministic_json_bytes(report))
    print(json.dumps(report["summary"], sort_keys=True))
    if (
        report["summary"]["execution_error"]
        or report["summary"]["harness_error"]
        or report["harness_errors"]
    ):
        return 2
    if report["summary"]["mismatch"]:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
