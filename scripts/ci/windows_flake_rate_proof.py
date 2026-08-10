#!/usr/bin/env python3
"""Repeat one cargo test selection N times and record every iteration.

A single pass cannot separate "fixed" from "won the race once", so this
instrument reports two observed rates and the per-iteration record behind them.
It supports or weakens a hypothesis; it does not establish causation, and its
own wording must not pretend otherwise.

It measures and never judges: a failing iteration is the datum, so the process
still exits 0. Three things are failures instead, because each one produces a
number that describes something other than what it claims:

- exit 2, the subject would not build;
- exit 40, an iteration executed no test at all;
- exit 41, a load arm lost its load, so an iteration labelled loaded was not.

That last one matters most. If load can vanish without the run failing, a clean
result means "no load" as readily as "no defect", and the instrument reports a
confident measurement of nothing.

The manifest binds the numbers to the head SHA, workflow SHA, repository, ref,
run and attempt, and digests the subject and the instrument. A rate carries only
to the identical `head_sha`: see REUSE_RULE.
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
import threading
import time
from pathlib import Path

SCHEMA = "assay.windows-flake-rate-proof/v2"

# Deliberately narrow, in the shape of the delegated lane's ceiling but making
# none of its claims. This instrument runs on a GitHub-hosted `windows-latest`
# image: there is no privileged or dedicated host, no OIDC attestation of the
# machine, and no verifier consuming this artifact.
CLAIM_CEILING = "hosted_windows_flake_rate_measurement_only_not_a_verdict"
NON_CLAIMS = [
    "not a gate: no required context consumes this artifact and no check is discharged by it",
    "no privileged, dedicated or attested host: GitHub-hosted windows-latest only",
    "no causal claim: arms run on separate hosted machines and these sample sizes do not"
    " separate a load effect from machine or scheduling variation",
    "no claim that a measured selection is fixed, only the rates the recorded iterations produced",
    "no claim about any head SHA, toolchain or image version other than the ones recorded here",
    "no claim about macOS or Linux behaviour",
]
# The subject closure is larger than any file list the instrument can honestly
# assert: the test target, the manifests, the lockfile and cargo's own behaviour
# all move the number. So reuse is pinned to the commit rather than to a
# curated set of paths that would silently carry a rate across changed content.
REUSE_RULE = "identical head_sha, instrument digests, toolchain and runner image version"

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

# Recorded as provenance for the reader, not as a reuse key: see REUSE_RULE.
SUBJECT_PATHS = ("tests/support/bounded_process.rs",)
INSTRUMENT_PATHS = (
    "scripts/ci/windows_flake_rate_proof.py",
    "scripts/ci/windows_flake_rate_proof_selftest.py",
    ".github/workflows/windows-flake-rate-proof.yml",
)

# How often load liveness is sampled, and how much of an iteration must be
# covered before the iteration counts as loaded at all.
LOAD_SAMPLE_INTERVAL_S = 0.25
MIN_LOAD_ALIVE_FRACTION = 0.95

EXIT_INSTRUMENT_BROKEN = 2
EXIT_COULD_NOT_MEASURE = 40
EXIT_LOAD_ABSENT = 41


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


def kill_tree(pid: int) -> None:
    """Kill a timed-out iteration's whole tree, bounded.

    `cargo test` is the parent of the test binary, which is the parent of
    whatever the test spawned, so killing the invocation alone would leave the
    measurement's load behind and poison every later iteration.
    """
    if sys.platform == "win32":
        run(["taskkill", "/T", "/F", "/PID", str(pid)], timeout=60)
    else:
        run(["pkill", "-KILL", "-P", str(pid)], timeout=60)
        run(["kill", "-KILL", str(pid)], timeout=60)


def run_with_ceiling(command: list[str], ceiling_s: float) -> tuple[int | None, str, bool]:
    """Return (exit code or None when timed out, combined output, timed_out)."""
    process = subprocess.Popen(
        command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True
    )
    try:
        out, err = process.communicate(timeout=ceiling_s)
        return process.returncode, out + err, False
    except subprocess.TimeoutExpired:
        kill_tree(process.pid)
        try:
            out, err = process.communicate(timeout=60)
        except subprocess.TimeoutExpired:
            process.kill()
            out, err = "", ""
        return None, (out or "") + (err or ""), True


def parse_iteration(output: str, timed_out: bool) -> dict:
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
        "timed_out": timed_out,
        # A timeout measured something: the selection did not finish inside its
        # ceiling. Executing no test at all measured nothing, and that must not
        # be counted as a clean run.
        "could_not_measure": not timed_out and (not verdicts or passed + failed == 0),
        "failed_tests": sorted(set(FAILED_TEST.findall(output))),
        "panics": panics,
    }


class LoadSupervisor:
    """Keeps the concurrent load alive and records how much of each iteration it covered.

    Checking liveness only between iterations would let the load exit during a
    measured test and leave the rest of it unloaded while still labelled loaded.
    So a thread samples continuously, restarts as often as needed -- no hidden
    cap -- and reports per-iteration coverage that the caller fails closed on.
    """

    def __init__(self, command: list[str] | None, interval_s: float = LOAD_SAMPLE_INTERVAL_S) -> None:
        self.command = command
        self.interval_s = interval_s
        self.starts = 0
        self._process: subprocess.Popen | None = None
        self._lock = threading.Lock()
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self._samples = 0
        self._alive = 0

    def _spawn(self) -> None:
        self._process = subprocess.Popen(
            self.command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
        )
        self.starts += 1

    def _supervise(self) -> None:
        while not self._stop.is_set():
            alive = self._process is not None and self._process.poll() is None
            with self._lock:
                self._samples += 1
                self._alive += 1 if alive else 0
            # Sample before restarting, so a gap is visible rather than papered
            # over by the restart that closes it.
            if not alive:
                self._spawn()
            self._stop.wait(self.interval_s)

    def start(self) -> None:
        if self.command is None:
            return
        self._spawn()
        self._thread = threading.Thread(target=self._supervise, daemon=True)
        self._thread.start()

    def begin_iteration(self) -> None:
        with self._lock:
            self._samples = 0
            self._alive = 0

    def iteration_coverage(self) -> dict:
        if self.command is None:
            return {"expected": False, "samples": 0, "alive_fraction": None, "gap": False}
        with self._lock:
            samples, alive = self._samples, self._alive
        fraction = round(alive / samples, 4) if samples else 0.0
        return {
            "expected": True,
            "samples": samples,
            "alive_fraction": fraction,
            # No sample at all is not "fully covered": it is no evidence of load.
            "gap": samples == 0 or fraction < MIN_LOAD_ALIVE_FRACTION,
        }

    def stop(self) -> None:
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=30)
        if self._process is not None and self._process.poll() is None:
            kill_tree(self._process.pid)


def manifest(ref_input: str, workflow_path: str) -> dict:
    server = os.environ.get("GITHUB_SERVER_URL", "")
    repository = os.environ.get("GITHUB_REPOSITORY", "")
    run_id = os.environ.get("GITHUB_RUN_ID", "")
    return {
        "claim_ceiling": CLAIM_CEILING,
        "non_claims": NON_CLAIMS,
        "reuse_rule": REUSE_RULE,
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


def summarize(iterations: list[dict], requested: int) -> dict:
    measured = [record for record in iterations if not record["could_not_measure"]]
    failed = [
        record
        for record in measured
        if record["timed_out"] or record["exit_code"] not in (0, None)
    ]
    per_test: dict[str, int] = {}
    for record in failed:
        for name in record["failed_tests"]:
            per_test[name] = per_test.get(name, 0) + 1
    return {
        "requested": requested,
        "measured": len(measured),
        "could_not_measure": [r["index"] for r in iterations if r["could_not_measure"]],
        "timed_out": [r["index"] for r in iterations if r["timed_out"]],
        "load_gaps": [r["index"] for r in iterations if r["load"]["gap"]],
        "failed_runs": len(failed),
        "observed_failure_rate": (
            round(len(failed) / len(measured), 4) if measured else None
        ),
        "failures_by_test": per_test,
    }


def instrument_verdict(summary: dict, completed: int) -> tuple[int, str]:
    """Decide whether the measurement describes what it claims to.

    Kept separate from the loop that produces it so the fail-closed rule has one
    home and can be pinned by the self-test without running cargo.
    """
    if summary["could_not_measure"] or completed != summary["requested"]:
        return (
            EXIT_COULD_NOT_MEASURE,
            "ERROR: iterations that executed no test: "
            f"{summary['could_not_measure']}; a run that could not measure is not a run that "
            "measured zero failures",
        )
    if summary["load_gaps"]:
        return (
            EXIT_LOAD_ABSENT,
            f"ERROR: iterations labelled loaded ran without it: {summary['load_gaps']}; "
            "an unloaded iteration cannot answer a question about load",
        )
    return 0, ""


def build_argument_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ref-input", default="")
    parser.add_argument("--workflow-path", default=".github/workflows/windows-flake-rate-proof.yml")
    parser.add_argument("--package", required=True)
    parser.add_argument("--test-target", required=True)
    parser.add_argument("--filter", default="")
    parser.add_argument("--iterations", type=int, required=True)
    parser.add_argument("--iteration-ceiling-s", type=float, default=300.0)
    parser.add_argument("--load", choices=["none", "workspace-tests"], default="none")
    parser.add_argument("--out", required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_argument_parser().parse_args(argv)

    if args.iterations < 1 or args.iteration_ceiling_s <= 0:
        print("ERROR: iterations and the per-iteration ceiling must be positive", file=sys.stderr)
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

    # Build the load arm's own selection before measuring, or the first minutes
    # of the arm are a compile storm rather than the test concurrency the arm is
    # supposed to reproduce. Measured on run 31417348260: iteration 2 breached a
    # 300s ceiling and iteration 3 took 243s, in both the baseline and the
    # candidate, which is the build starving the box and not either tree.
    if load_command is not None:
        prebuild = run([*load_command, "--no-run"])
        if prebuild.returncode != 0:
            sys.stderr.write(prebuild.stdout)
            sys.stderr.write(prebuild.stderr)
            print("ERROR: the instrument could not build its load arm", file=sys.stderr)
            return EXIT_INSTRUMENT_BROKEN

    load = LoadSupervisor(load_command)
    load.start()
    iterations: list[dict] = []
    try:
        for index in range(1, args.iterations + 1):
            load.begin_iteration()
            started = time.monotonic()
            code, output, timed_out = run_with_ceiling(measured, args.iteration_ceiling_s)
            record = {
                "index": index,
                "exit_code": code,
                "duration_s": round(time.monotonic() - started, 3),
                "load": load.iteration_coverage(),
                **parse_iteration(output, timed_out),
            }
            iterations.append(record)
            print(
                f"iteration {index}/{args.iterations}: exit={code} timed_out={timed_out} "
                f"failed={record['failed']} load={record['load']['alive_fraction']} "
                f"in {record['duration_s']}s {record['failed_tests']}",
                flush=True,
            )
    finally:
        load.stop()

    summary = summarize(iterations, args.iterations)
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
            "iteration_ceiling_s": args.iteration_ceiling_s,
        },
        "load": {
            "mode": args.load,
            "command": load_command,
            "starts": load.starts,
            "prebuilt": load_command is not None,
        },
        "iterations": iterations,
        "summary": summary,
    }
    Path(args.out).write_text(json.dumps(proof, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))

    code, message = instrument_verdict(summary, len(iterations))
    if code != 0:
        print(message, file=sys.stderr)
    return code


if __name__ == "__main__":
    sys.exit(main())
