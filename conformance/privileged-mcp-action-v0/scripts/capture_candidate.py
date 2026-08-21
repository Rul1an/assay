#!/usr/bin/env python3
"""Run a candidate over the opaque cases and record what it emitted. No oracle here.

This is the phase hostile code runs in. It takes a clean-room pack and an
entrypoint; it takes no manifest, no expectations and no canonical corpus, so a
candidate that escapes its process bounds has nothing on this host to read. The
capture it writes is scored later, elsewhere, by `score_candidate.py --capture`.

What that separation does and does not buy is stated in `CAPTURE_NON_CLAIMS`:
the oracle is absent from this host, and the capture is still unauthenticated.
"""

from __future__ import annotations

import argparse
import gzip
import io
import json
import shlex
import sys
import tarfile
import tempfile
from pathlib import Path, PurePosixPath
from collections.abc import Callable
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bounded_process import ProcessCaptureError, ProcessLimitError, run_bounded  # noqa: E402
from artifact_io import (  # noqa: E402
    content_sha256 as sha256,
    render_deterministic_json_bytes,
    write_regular_file_atomically,
)
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "adequacy"))
import published_rows  # noqa: E402
from capture_format import (  # noqa: E402
    CAPTURE_NON_CLAIMS,
    CAPTURE_SCHEMA,
    STATE_CANDIDATE_ERROR,
    STATE_CAPTURE_ERROR,
    SUITE,
    add_identity_arguments,
    identity_from_args,
    normative_surface,
    observe,
    observe_error,
    validate_capture,
)
from pack_format import BoundedReader, opaque_case_id  # noqa: E402
from validate_run_record import (  # noqa: E402
    EXPECTED_CASE_COUNT,
    FULL_SHA,
    PROFILE,
    SHA256,
    validate_normative_surface,
)

PACK_ROOT = "privileged-mcp-action-v0"
PACK_SCHEMA = "assay.privileged_mcp_action.clean_room_pack.v0"
MAX_PACK_BYTES = 100 * 1024 * 1024
MAX_MEMBER_BYTES = 16 * 1024 * 1024
# README, cases.json, descriptor.json, spec.md, plus the two canonicalization members added in
# candidate.4 (#1990). Counted rather than derived so a pack that grew a member fails here instead
# of being accepted because the scorer was taught to expect whatever it was handed.
EXPECTED_PACK_MEMBERS = EXPECTED_CASE_COUNT + 6
MAX_PACK_ARCHIVE_BYTES = 32 * 1024 * 1024
MAX_MEMBER_NAME_BYTES = 512
MAX_OUTPUT_BYTES = 1024 * 1024


class CandidateError(ValueError):
    pass


class HarnessError(RuntimeError):
    pass


def read_pack_bytes(path: Path) -> bytes:
    return published_rows.read_regular_file(Path(path), limit=MAX_PACK_BYTES)


def sha256_file(path: Path) -> str:
    return sha256(read_pack_bytes(path))


def read_member(archive: tarfile.TarFile, member: tarfile.TarInfo) -> bytes:
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


def load_pack_from_bytes(data: bytes, destination: Path) -> dict[str, Any]:
    if len(data) > MAX_PACK_BYTES:
        raise ValueError(f"pack exceeds {MAX_PACK_BYTES} bytes")
    files: dict[str, bytes] = {}
    expanded_bytes = 0
    with io.BytesIO(data) as raw:
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
        f"{PACK_ROOT}/canonicalization/README.md",
        f"{PACK_ROOT}/canonicalization/rfc8785-vectors.json",
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


def load_pack_with_digest(pack: Path, destination: Path) -> tuple[dict[str, Any], str]:
    data = read_pack_bytes(pack)
    return load_pack_from_bytes(data, destination), sha256(data)


def load_pack(pack: Path, destination: Path) -> dict[str, Any]:
    loaded, _digest = load_pack_with_digest(pack, destination)
    return loaded


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


def run_candidate(command: list[str], bundle: Path, timeout_seconds: int) -> dict[str, Any]:
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


_DEFAULT_CANDIDATE_RUNNER = run_candidate


def capture_observations(
    pack: dict[str, Any],
    command: list[str],
    timeout_seconds: int,
    *,
    candidate_runner: Callable[[list[str], Path, int], dict[str, Any]] = _DEFAULT_CANDIDATE_RUNNER,
) -> list[dict[str, Any]]:
    """Execute every opaque case and record one observation each, in pack order."""
    runner = (
        run_candidate
        if candidate_runner is _DEFAULT_CANDIDATE_RUNNER
        else candidate_runner
    )
    observations = []
    for case in pack["cases"]:
        case_id = case["id"]
        digest = case["sha256"]
        try:
            execution = runner(command, Path(case["_local_path"]), timeout_seconds)
        except HarnessError as error:
            observations.append(
                observe_error(case_id, digest, STATE_CAPTURE_ERROR, str(error))
            )
        except (OSError, CandidateError) as error:
            observations.append(
                observe_error(case_id, digest, STATE_CANDIDATE_ERROR, str(error))
            )
        else:
            observations.append(
                observe(
                    case_id,
                    digest,
                    execution["report"],
                    execution["exit_code"],
                    execution["stderr_present"],
                )
            )
    return observations


def build_capture(
    pack: dict[str, Any],
    pack_digest: str,
    observations: list[dict[str, Any]],
    implementation: dict[str, Any],
) -> dict[str, Any]:
    return {
        "schema": CAPTURE_SCHEMA,
        "profile": PROFILE,
        "suite": SUITE,
        "pack_sha256": pack_digest,
        "pack_declared_source_commit": pack["declared_source_commit"],
        "source_corpus_digest": pack["source_corpus_digest"],
        "rendered_set_digest": pack["rendered_set_digest"],
        "implementation": implementation,
        "observations": observations,
        "capture_non_claims": list(CAPTURE_NON_CLAIMS),
    }


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return parsed


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--pack", type=Path, required=True)
    parser.add_argument("--entrypoint", required=True)
    add_identity_arguments(parser)
    parser.add_argument("--timeout-seconds", type=positive_int, default=30)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
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
        observations = capture_observations(pack, command, args.timeout_seconds)

    capture = build_capture(pack, pack_digest, observations, identity_from_args(args))
    try:
        validate_capture(capture)
    except ValueError as error:
        print(f"capture host produced an invalid capture: {error}", file=sys.stderr)
        return 2
    args.output.parent.mkdir(parents=True, exist_ok=True)
    write_regular_file_atomically(args.output, render_deterministic_json_bytes(capture))
    print(f"captured {len(observations)} observations")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
