#!/usr/bin/env python3
"""Repeat one cargo test selection N times and record every iteration.

A single pass cannot separate "fixed" from "won the race once", so this
instrument reports a rate and the per-iteration record behind it.

It measures and never judges: a failing iteration is the datum, so the process
still exits 0. Two things are failures instead. A subject that will not build
exits 2. An iteration that executed no test -- a filter that matched nothing, a
binary that never ran -- exits 40, mirroring the delegated lane where an
environment skip is a failure rather than a neutral outcome. A run that could
not measure is not a run that measured zero failures, and for a failure-rate
instrument that is the most likely way to lie quietly.

The manifest binds the numbers to the head SHA, workflow SHA, repository, ref,
run and attempt, and digests the files that define both subject and measurement,
so a rate is addressable rather than asserted. It also states its own claim
ceiling: see CLAIM_CEILING below.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import subprocess
import sys
import time
from pathlib import Path

SCHEMA = "assay.windows-flake-rate-proof/v1"

# Deliberately narrow, in the shape of the delegated lane's ceiling but making
# none of its claims. This instrument runs on a GitHub-hosted `windows-latest`
# image: there is no privileged or dedicated host, no OIDC attestation of the
# machine, and no verifier consuming this artifact.
CLAIM_CEILING = "hosted_windows_flake_rate_measurement_only_not_a_verdict"
NON_CLAIMS = [
    "not a gate: no required context consumes this artifact and no check is discharged by it",
    "no privileged, dedicated or attested host: GitHub-hosted windows-latest only",
    "no claim that a measured selection is fixed, only the rate the recorded iterations produced",
    "no claim about any image version, toolchain or SHA other than the ones recorded here",
    "no claim about macOS or Linux behaviour",
]

FAILED_TEST = re.compile(r"^test (\S+) \.\.\. FAILED\s*$", re.MULTILINE)
SUMMARY = re.compile(
    r"^test result: (?P<verdict>\w+)\. (?P<passed>\d+) passed; (?P<failed>\d+) failed;"
    r" (?P<ignored>\d+) ignored",
    re.MULTILINE,
)
PANIC = re.compile(
    r"^thread '(?P<test>[^']+)'.*panicked at (?P<site>[^\n]*)\n(?P<message>[^\n]*)",
    re.MULTILINE,
)
MAX_MESSAGE_CHARS = 600

# The files whose content the rate is a measurement of. Recording their blob
# OIDs is what lets a rate carry to a later head: it carries only when these are
# identical, which is the delegated lane's content-addressed rule applied to a
# measurement instead of a gate.
SUBJECT_PATHS = ("tests/support/bounded_process.rs",)
# The files that define the measurement itself. A rate taken with a different
# instrument is a different rate.
INSTRUMENT_PATHS = (
    "scripts/ci/windows_flake_rate_proof.py",
    ".github/workflows/windows-flake-rate-proof.yml",
)

EXIT_INSTRUMENT_BROKEN = 2
EXIT_COULD_NOT_MEASURE = 40


def run(command: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(command, capture_output=True, text=True, check=False, **kwargs)


def git_output(args: list[str]) -> str:
    result = run(["git", *args])
    return result.stdout.strip() if result.returncode == 0 else ""


def digest_of(path: str) -> dict:
    entry = {"blob_oid": git_output(["rev-parse", f"HEAD:{path}"])}
    file = Path(path)
    if file.is_file():
        entry["sha256"] = hashlib.sha256(file.read_bytes()).hexdigest()
    return entry


def manifest(ref_input: str, workflow_path: str) -> dict:
    server = os.environ.get("GITHUB_SERVER_URL", "")
    repository = os.environ.get("GITHUB_REPOSITORY", "")
    run_id = os.environ.get("GITHUB_RUN_ID", "")
    return {
        "claim_ceiling": CLAIM_CEILING,
        "non_claims": NON_CLAIMS,
        "ref_input": ref_input,
        "head_sha": git_output(["rev-parse", "HEAD"]),
        "worktree_clean": git_output(["status", "--porcelain"]) == "",
        "repository": repository,
        "ref": os.environ.get("GITHUB_REF", ""),
        "workflow_name": os.environ.get("GITHUB_WORKFLOW", ""),
        "workflow_path": workflow_path,
        "workflow_sha": os.environ.get("GITHUB_WORKFLOW_SHA", ""),
        "run_id": run_id,
        "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT", ""),
        "run_url": f"{server}/{repository}/actions/runs/{run_id}" if server and repository else "",
        "subject_digests": {path: digest_of(path) for path in SUBJECT_PATHS},
        "instrument_digests": {path: digest_of(path) for path in INSTRUMENT_PATHS},
    }


def runner_facts() -> dict:
    return {
        # Set by GitHub-hosted runners. If a failure turns out to be an image
        # rollout, this field is what proves it.
        "image_os": os.environ.get("ImageOS", ""),
        "image_version": os.environ.get("ImageVersion", ""),
        "runner_name": os.environ.get("RUNNER_NAME", ""),
        "platform": platform.platform(),
        "processor_count": os.cpu_count(),
    }


def toolchain_facts() -> dict:
    return {
        "rustc": run(["rustc", "--version"]).stdout.strip(),
        "cargo": run(["cargo", "--version"]).stdout.strip(),
    }


def parse_iteration(output: str) -> dict:
    passed = failed = ignored = 0
    verdicts = []
    for match in SUMMARY.finditer(output):
        passed += int(match.group("passed"))
        failed += int(match.group("failed"))
        ignored += int(match.group("ignored"))
        verdicts.append(match.group("verdict"))
    panics = [
        {
            "test": match.group("test"),
            "site": match.group("site").strip(),
            "message": match.group("message").strip()[:MAX_MESSAGE_CHARS],
        }
        for match in PANIC.finditer(output)
    ]
    return {
        "passed": passed,
        "failed": failed,
        "ignored": ignored,
        "verdicts": verdicts,
        # No summary line, or a summary reporting nothing executed, means this
        # iteration measured nothing. It must not be counted as a clean run.
        "could_not_measure": not verdicts or passed + failed == 0,
        "failed_tests": sorted(set(FAILED_TEST.findall(output))),
        "panics": panics,
    }


class BackgroundLoad:
    """Keeps a concurrent workspace test run alive for the whole measurement.

    The load arm exists so the load hypothesis can be tested directly rather
    than inferred: same SHA, same image, once alone and once under the
    concurrency an ordinary `cargo test` produces.
    """

    def __init__(self, command: list[str] | None, max_starts: int = 20) -> None:
        self.command = command
        self.max_starts = max_starts
        self.starts = 0
        self.process: subprocess.Popen | None = None

    def keep_alive(self) -> None:
        if self.command is None or self.starts >= self.max_starts:
            return
        if self.process is None or self.process.poll() is not None:
            self.starts += 1
            self.process = subprocess.Popen(
                self.command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
            )

    def stop(self) -> None:
        if self.process is not None and self.process.poll() is None:
            self.process.kill()
            self.process.wait(timeout=60)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ref-input", default="")
    parser.add_argument("--workflow-path", default=".github/workflows/windows-flake-rate-proof.yml")
    parser.add_argument("--package", required=True)
    parser.add_argument("--test-target", required=True)
    parser.add_argument("--filter", default="")
    parser.add_argument("--iterations", type=int, required=True)
    parser.add_argument("--load", choices=["none", "workspace-tests"], default="none")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    if args.iterations < 1:
        print("ERROR: iterations must be at least 1", file=sys.stderr)
        return EXIT_INSTRUMENT_BROKEN

    measured = ["cargo", "test", "--locked", "-p", args.package, "--test", args.test_target]
    if args.filter:
        measured.append(args.filter)
    # The Windows leg of CI runs this selection, so the load arm reproduces the
    # concurrency the measured tests actually meet there.
    load_command = (
        [
            "cargo", "test", "--locked", "--workspace",
            "--exclude", "assay-ebpf", "--exclude", "assay-it",
            "--exclude", "assay-monitor", "--exclude", "assay-cli",
        ]
        if args.load == "workspace-tests"
        else None
    )

    build = run([*measured, "--no-run"])
    if build.returncode != 0:
        sys.stderr.write(build.stdout)
        sys.stderr.write(build.stderr)
        print("ERROR: the instrument could not build its subject", file=sys.stderr)
        return EXIT_INSTRUMENT_BROKEN

    load = BackgroundLoad(load_command)
    iterations = []
    try:
        for index in range(1, args.iterations + 1):
            load.keep_alive()
            started = time.monotonic()
            result = run(measured)
            elapsed = round(time.monotonic() - started, 3)
            record = {
                "index": index,
                "exit_code": result.returncode,
                "duration_s": elapsed,
                **parse_iteration(result.stdout + result.stderr),
            }
            iterations.append(record)
            print(
                f"iteration {index}/{args.iterations}: exit={record['exit_code']} "
                f"failed={record['failed']} in {elapsed}s {record['failed_tests']}",
                flush=True,
            )
    finally:
        load.stop()

    unmeasured = [record["index"] for record in iterations if record["could_not_measure"]]
    failed_runs = [
        record
        for record in iterations
        if record["exit_code"] != 0 and not record["could_not_measure"]
    ]
    measured_runs = [record for record in iterations if not record["could_not_measure"]]
    per_test: dict[str, int] = {}
    for record in failed_runs:
        for name in record["failed_tests"]:
            per_test[name] = per_test.get(name, 0) + 1

    proof = {
        "schema": SCHEMA,
        "manifest": manifest(args.ref_input, args.workflow_path),
        "runner": runner_facts(),
        "toolchain": toolchain_facts(),
        "subject": {
            "package": args.package,
            "test_target": args.test_target,
            "filter": args.filter,
            "command": measured,
        },
        "load": {"mode": args.load, "command": load_command, "starts": load.starts},
        "iterations": iterations,
        "summary": {
            "requested": args.iterations,
            "measured": len(measured_runs),
            "could_not_measure": unmeasured,
            "failed_runs": len(failed_runs),
            "failure_rate": (
                round(len(failed_runs) / len(measured_runs), 4) if measured_runs else None
            ),
            "failures_by_test": per_test,
        },
    }
    Path(args.out).write_text(json.dumps(proof, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(proof["summary"], indent=2))

    if unmeasured or len(iterations) != args.iterations:
        print(
            f"ERROR: {len(unmeasured) or args.iterations - len(iterations)} iteration(s) measured "
            "nothing; a run that could not measure is not a run that measured zero failures",
            file=sys.stderr,
        )
        return EXIT_COULD_NOT_MEASURE
    return 0


if __name__ == "__main__":
    sys.exit(main())
