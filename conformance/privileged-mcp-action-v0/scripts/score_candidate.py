#!/usr/bin/env python3
"""Run a candidate implementation against opaque cases, then score normative results."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import re
import shlex
import sys
import tarfile
import tempfile
from pathlib import Path, PurePosixPath
from typing import Any
from urllib.parse import urlparse

from bounded_process import ProcessCaptureError, ProcessLimitError, run_bounded
from pack_format import (
    BoundedReader,
    opaque_case_id,
    opaque_run_id,
    ordered_vectors,
    rewrite_bundle_stream_identity,
)
from validate_run_record import (
    RUN_NON_CLAIMS,
    validate_normative_surface,
    validate_run_record,
)

PACK_ROOT = "privileged-mcp-action-v0"
PACK_SCHEMA = "assay.privileged_mcp_action.clean_room_pack.v0"
RUN_SCHEMA = "assay.privileged_mcp_action.conformance_run.v0"
PROFILE = "privileged-mcp-action/v0"
REPORT_SCHEMA = "assay.privileged_mcp_action.verify.report.v0"
REPRODUCTION_MODES = (
    "blind_from_spec",
    "from_spec_then_conformance",
    "commissioned_clean_room",
    "other_disclosed",
)
MAX_PACK_BYTES = 100 * 1024 * 1024
MAX_MEMBER_BYTES = 16 * 1024 * 1024
EXPECTED_CASE_COUNT = 14
# spec.md, descriptor.json, cases.json, README.md. Derived rather than written out again:
# the pack member count and the case count are one fact, and holding them as two numbers is
# how the corpus grew to fourteen while a constant still said thirteen.
EXPECTED_PACK_MEMBERS = EXPECTED_CASE_COUNT + 4
MAX_PACK_ARCHIVE_BYTES = 32 * 1024 * 1024
MAX_MEMBER_NAME_BYTES = 512
MAX_OUTPUT_BYTES = 1024 * 1024
FULL_SHA = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^sha256:[0-9a-f]{64}$")


class CandidateError(ValueError):
    pass


class HarnessError(RuntimeError):
    pass


def sha256(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def read_member(
    archive: tarfile.TarFile,
    member: tarfile.TarInfo,
) -> bytes:
    if not member.isfile():
        raise ValueError(f"pack member is not a regular file: {member.name}")
    if member.size > MAX_MEMBER_BYTES:
        raise ValueError(f"pack member exceeds {MAX_MEMBER_BYTES} bytes: {member.name}")
    source = archive.extractfile(member)
    if source is None:
        raise ValueError(f"cannot read pack member: {member.name}")
    data = source.read(MAX_MEMBER_BYTES + 1)
    if len(data) > MAX_MEMBER_BYTES:
        raise ValueError(f"pack member expands past {MAX_MEMBER_BYTES} bytes: {member.name}")
    return data


def load_pack(pack: Path, destination: Path) -> dict[str, Any]:
    if pack.stat().st_size > MAX_PACK_BYTES:
        raise ValueError(f"pack exceeds {MAX_PACK_BYTES} bytes")
    files: dict[str, bytes] = {}
    expanded_bytes = 0
    with pack.open("rb") as raw:
        with gzip.GzipFile(fileobj=raw) as decoded:
            bounded = BoundedReader(decoded, MAX_PACK_ARCHIVE_BYTES)
            with tarfile.open(fileobj=bounded, mode="r|") as archive:
                for index_number, member in enumerate(archive):
                    if index_number >= EXPECTED_PACK_MEMBERS:
                        raise ValueError("pack contains surplus members")
                    if len(member.name.encode("utf-8")) > MAX_MEMBER_NAME_BYTES:
                        raise ValueError("pack member name is too long")
                    if member.name in files:
                        raise ValueError("pack contains duplicate member names")
                    expanded_bytes += member.size
                    if expanded_bytes > MAX_PACK_ARCHIVE_BYTES:
                        raise ValueError(
                            f"pack expands past {MAX_PACK_ARCHIVE_BYTES} bytes"
                        )
                    files[member.name] = read_member(archive, member)

    if len(files) != EXPECTED_PACK_MEMBERS:
        raise ValueError("pack member count is invalid")
    index_name = f"{PACK_ROOT}/cases.json"
    if index_name not in files:
        raise ValueError("pack is missing cases.json")
    index = json.loads(files[index_name])
    if index.get("schema") != PACK_SCHEMA or index.get("profile") != PROFILE:
        raise ValueError("pack schema or profile mismatch")
    if not FULL_SHA.fullmatch(index.get("declared_source_commit", "")):
        raise ValueError("pack declared_source_commit is not a full commit")
    if not SHA256.fullmatch(index.get("source_corpus_digest", "")):
        raise ValueError("pack source_corpus_digest is malformed")
    if not SHA256.fullmatch(index.get("rendered_set_digest", "")):
        raise ValueError("pack rendered_set_digest is malformed")
    cases = index.get("cases")
    if (
        not isinstance(cases, list)
        or len(cases) != EXPECTED_CASE_COUNT
        or index.get("case_count") != len(cases)
    ):
        raise ValueError("pack case count is invalid")

    expected_names = {
        f"{PACK_ROOT}/README.md",
        f"{PACK_ROOT}/cases.json",
        f"{PACK_ROOT}/descriptor.json",
        f"{PACK_ROOT}/spec.md",
    }
    seen_ids: set[str] = set()
    seen_hashes: set[str] = set()
    for index_number, case in enumerate(cases, start=1):
        case_id = case.get("id")
        relative = case.get("file")
        expected_hash = case.get("sha256")
        expected_id = opaque_case_id(index_number)
        if case_id != expected_id or case_id in seen_ids:
            raise ValueError("pack case ids must be ordered unique case numbers")
        if not isinstance(relative, str):
            raise ValueError(f"{case_id}: case path must be a string")
        path = PurePosixPath(relative)
        if (
            path.is_absolute()
            or ".." in path.parts
            or path.as_posix() != f"cases/{case_id}.bundle.tar.gz"
        ):
            raise ValueError(f"{case_id}: unsafe case path")
        if not SHA256.fullmatch(expected_hash or ""):
            raise ValueError(f"{case_id}: malformed case digest")
        member_name = f"{PACK_ROOT}/{path.as_posix()}"
        expected_names.add(member_name)
        data = files.get(member_name)
        if data is None:
            raise ValueError(f"{case_id}: pack member is missing")
        digest = sha256(data)
        if digest != expected_hash:
            raise ValueError(f"{case_id}: case digest mismatch")
        if digest in seen_hashes:
            raise ValueError(f"{case_id}: duplicate case bytes")
        seen_ids.add(case_id)
        seen_hashes.add(digest)
        output = destination / f"{case_id}.bundle.tar.gz"
        output.write_bytes(data)
        case["_local_path"] = str(output)

    if set(files) != expected_names:
        raise ValueError("pack contains missing or surplus members")
    rendered_set_digest = sha256(
        "".join(case["sha256"] + "\n" for case in cases).encode()
    )
    if rendered_set_digest != index["rendered_set_digest"]:
        raise ValueError("pack rendered_set_digest does not bind its cases")
    return index


def parse_candidate_report(stdout: bytes) -> dict[str, Any]:
    try:
        value = json.loads(stdout.decode("utf-8"))
    except (UnicodeDecodeError, ValueError, RecursionError) as error:
        raise CandidateError(f"stdout is not exactly one JSON document: {error}") from error
    if not isinstance(value, dict):
        raise CandidateError("candidate report must be a JSON object")
    integrity = value.get("bundle_integrity")
    if integrity not in {"pass", "fail"}:
        raise CandidateError("bundle_integrity must be pass or fail")
    if integrity == "fail":
        if "verdict" in value or "claims" in value:
            raise CandidateError("integrity-fail report must omit verdict and claims")
        return value
    verdict = value.get("verdict")
    if verdict not in {"valid", "invalid"}:
        raise CandidateError("pass report verdict must be valid or invalid")
    if verdict == "valid" and not isinstance(value.get("claims"), dict):
        raise CandidateError("valid report must carry a claims object")
    if verdict == "invalid" and "claims" in value:
        raise CandidateError("invalid report must omit claims")
    try:
        validate_normative_surface(normative_surface(value))
    except (KeyError, TypeError, ValueError) as error:
        raise CandidateError(f"candidate normative result is invalid: {error}") from error
    return value


def run_candidate(
    command: list[str],
    bundle: Path,
    timeout_seconds: int,
) -> dict[str, Any]:
    try:
        completed = run_bounded(
            [*command, str(bundle)],
            timeout_seconds=timeout_seconds,
            stdout_limit=MAX_OUTPUT_BYTES,
            stderr_limit=MAX_OUTPUT_BYTES,
        )
    except ProcessCaptureError as error:
        raise HarnessError(f"candidate output capture failed: {error}") from error
    except ProcessLimitError as error:
        raise CandidateError(f"candidate execution limit exceeded: {error}") from error
    except OSError as error:
        raise CandidateError(
            f"candidate process could not start ({type(error).__name__})"
        ) from error
    report = parse_candidate_report(completed.stdout)
    return {
        "exit_code": completed.returncode,
        "report": report,
        "stderr_present": bool(completed.stderr),
    }


def load_expectations(manifest_path: Path) -> tuple[dict[str, Any], str, str]:
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    expected_by_hash: dict[str, Any] = {}
    vectors = ordered_vectors(manifest["vectors"])
    for index, vector in enumerate(vectors, start=1):
        source = (manifest_path.parent / vector["file"]).read_bytes()
        if sha256(source) != vector["sha256"]:
            raise ValueError(f"canonical source digest mismatch for {vector['file']}")
        transformed = rewrite_bundle_stream_identity(
            source,
            opaque_run_id(index),
        )
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


def normative_surface(report: dict[str, Any]) -> dict[str, Any]:
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


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return parsed


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pack", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--entrypoint", required=True)
    parser.add_argument("--implementation-name", required=True)
    parser.add_argument(
        "--reproduction-mode",
        choices=REPRODUCTION_MODES,
        required=True,
    )
    parser.add_argument("--implementation-version")
    parser.add_argument("--implementation-source", required=True)
    parser.add_argument("--implementation-commit", required=True)
    parser.add_argument("--timeout-seconds", type=positive_int, default=30)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    command = shlex.split(args.entrypoint)
    if not command:
        print("--entrypoint must not be empty", file=sys.stderr)
        return 2
    implementation_source = urlparse(args.implementation_source)
    if (
        implementation_source.scheme not in {"http", "https"}
        or not implementation_source.netloc
    ):
        print("--implementation-source must be an absolute HTTP(S) URL", file=sys.stderr)
        return 2
    if not FULL_SHA.fullmatch(args.implementation_commit):
        print("--implementation-commit must be a full lowercase 40-hex commit", file=sys.stderr)
        return 2

    with tempfile.TemporaryDirectory() as tmp:
        try:
            pack = load_pack(args.pack, Path(tmp))
            pack_digest = sha256_file(args.pack)
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

        # Snapshot the oracle before candidate code runs so it cannot rewrite the
        # comparison inputs. The snapshot is scorer-private and is never passed to
        # the candidate; comparisons still happen only after every opaque execution.
        observations = []
        for case in pack["cases"]:
            try:
                execution = run_candidate(
                    command,
                    Path(case["_local_path"]),
                    args.timeout_seconds,
                )
                observations.append({"case": case, "execution": execution})
            except HarnessError as error:
                observations.append({"case": case, "harness_error": str(error)})
            except (OSError, CandidateError) as error:
                observations.append({"case": case, "error": str(error)})

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
            "total": len(observations),
            "match": 0,
            "mismatch": 0,
            "execution_error": 0,
            "harness_error": 0,
            "review_warnings": 0,
        }
        for observation in observations:
            case = observation["case"]
            result: dict[str, Any] = {
                "case_id": case["id"],
                "input_sha256": case["sha256"],
            }
            if "harness_error" in observation:
                result.update(
                    status="harness_error",
                    error=observation["harness_error"],
                )
                counts["harness_error"] += 1
                cases.append(result)
                continue
            if "error" in observation:
                result.update(status="execution_error", error=observation["error"])
                counts["execution_error"] += 1
                cases.append(result)
                continue
            execution = observation["execution"]
            observed = normative_surface(execution["report"])
            expected = expected_by_hash.get(case["sha256"])
            if expected is None:
                result.update(
                    status="harness_error",
                    error="opaque case is absent from canonical expectations",
                )
                counts["harness_error"] += 1
                cases.append(result)
                continue
            matched = observed == expected
            result.update(
                status="match" if matched else "mismatch",
                observed=observed,
                exit_code=execution["exit_code"],
                stderr_present=execution["stderr_present"],
            )
            counts[result["status"]] += 1
            warnings = []
            if (
                observed.get("bundle_integrity") == "pass"
                and observed.get("verdict") == "invalid"
                and not has_reviewer_reason(execution["report"])
            ):
                warnings.append("reject_reason_missing")
            if execution["report"].get("schema") != REPORT_SCHEMA:
                warnings.append("report_schema_missing_or_unexpected")
            if execution["report"].get("profile") != PROFILE:
                warnings.append("report_profile_missing_or_unexpected")
            if warnings:
                result["review_warnings"] = warnings
                counts["review_warnings"] += len(warnings)
            cases.append(result)

    report = {
        "schema": RUN_SCHEMA,
        "profile": PROFILE,
        "source_corpus_digest": pack["source_corpus_digest"],
        "rendered_set_digest": pack["rendered_set_digest"],
        "pack_sha256": pack_digest,
        "pack_declared_source_commit": pack["declared_source_commit"],
        "pack_provenance_verification": "not_performed_by_scorer",
        "implementation": {
            "name": args.implementation_name,
            "version": args.implementation_version,
            "source": args.implementation_source,
            "commit": args.implementation_commit,
            "reproduction_mode": args.reproduction_mode,
        },
        "summary": counts,
        "harness_errors": harness_errors,
        "cases": cases,
        "non_claims": list(RUN_NON_CLAIMS),
    }
    try:
        validate_run_record(report)
    except (KeyError, TypeError, ValueError) as error:
        print(f"scorer produced an invalid run record: {error}", file=sys.stderr)
        return 2
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(counts, sort_keys=True))
    if counts["execution_error"] or counts["harness_error"] or harness_errors:
        return 2
    if counts["mismatch"]:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
