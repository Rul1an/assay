#!/usr/bin/env python3
"""Run every published Assay conformance corpus and grade each one.

Standard library only. No Assay import, no pip install, no network.

WHY THE GRADING HAS THREE VALUES AND NOT TWO
--------------------------------------------
A boolean cannot tell a reader whether a check ran and disagreed or was never
reached, and those need different repairs. So a suite grades:

    proved    the suite ran and agreed with its own pinned expectations
    false     the suite ran and DISAGREED -- a real, reportable divergence
    unproved  an execution condition stopped the evaluation

`unproved` is only ever produced by an execution state this runner observed --
a missing toolchain, an unreadable corpus, a non-zero exit with no parseable
report. It is NEVER inferred from a primary check that ran and failed, because
that is a stronger claim than the run established. Where a run mixes states,
the worst one wins.

WHY SOME SUITES REPORT "did not run" AND THAT IS NOT A PASS
-----------------------------------------------------------
`privileged-mcp-action-v0` is a clean-room gate: scoring it REQUIRES a
candidate implementation supplied with --entrypoint. There is no self-run and
there is deliberately no self-run, because a corpus that scores itself answers
a question nobody asked. It therefore reports `needs_candidate` every time.

"The suite agreed" and "nothing exercised the suite" must never print
identically, so non-run states are declared, counted, and shown in the summary
rather than omitted from it.
"""

from __future__ import annotations

import argparse
import json
import re
import os
import signal
import subprocess
import tempfile
import time
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# Grades a run can produce.
PROVED, FALSE, UNPROVED = "proved", "false", "unproved"
# Declared non-run states. Never inferred, never counted as agreement.
NEEDS_CANDIDATE, NOT_SELECTED, EXTERNAL = "needs_candidate", "not_selected", "external"

RANK = {PROVED: 0, NEEDS_CANDIDATE: 0, NOT_SELECTED: 0, EXTERNAL: 0, UNPROVED: 1, FALSE: 2}


# Output ceiling per child process. Both children can emit arbitrary output and
# a timeout does not bound memory, so the cap is applied while the process runs
# rather than after it exits.
OUTPUT_CAP_BYTES = 4 * 1024 * 1024


class _OutputTooLarge(Exception):
    """A child exceeded OUTPUT_CAP_BYTES; its output is not materialized."""


def _run_capped(cmd: list[str], cwd: Path, timeout: int) -> subprocess.CompletedProcess:
    """subprocess.run with a ceiling on how much output is ever held.

    Streams both pipes to temporary files, polls their combined size while the
    child runs, and kills it the moment the cap is crossed. Only the first
    OUTPUT_CAP_BYTES are ever read into memory.
    """
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        # A descendant that inherits stdout/stderr keeps writing after proc.kill()
        # and walks straight through the ceiling, so the child leads its own session
        # and the whole group is stopped and reaped.
        proc = subprocess.Popen(cmd, cwd=str(cwd), stdout=out, stderr=err,
                                start_new_session=True)

        def _kill_tree() -> None:
            try:
                os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
            except (ProcessLookupError, PermissionError, OSError):
                proc.kill()
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                pass
        deadline = time.monotonic() + timeout
        try:
            while True:
                try:
                    proc.wait(timeout=0.25)
                    break
                except subprocess.TimeoutExpired:
                    pass
                if out.tell() + err.tell() > OUTPUT_CAP_BYTES:
                    _kill_tree()
                    raise _OutputTooLarge()
                if time.monotonic() > deadline:
                    _kill_tree()
                    raise subprocess.TimeoutExpired(cmd, timeout)
        finally:
            # Reap the group even on the clean path: the leader can exit while a
            # descendant it spawned is still holding the inherited handles.
            _kill_tree()
        if out.tell() + err.tell() > OUTPUT_CAP_BYTES:
            raise _OutputTooLarge()
        out.seek(0)
        err.seek(0)
        return subprocess.CompletedProcess(
            cmd, proc.returncode,
            out.read(OUTPUT_CAP_BYTES).decode("utf-8", "replace"),
            err.read(OUTPUT_CAP_BYTES).decode("utf-8", "replace"),
        )


def _stdlib_jsonrpc(suite: dict) -> tuple[str, str]:
    """examples/mcp-jsonrpc-id-conformance: `check.py reproduce`, offline."""
    d = REPO / suite["path"]
    if not (d / "check.py").is_file():
        return UNPROVED, "check.py absent at %s" % suite["path"]
    try:
        p = _run_capped([sys.executable, "check.py", "reproduce"], d, timeout=120)
    except _OutputTooLarge:
        return UNPROVED, "output exceeded %d bytes; not materialized" % OUTPUT_CAP_BYTES
    except (OSError, subprocess.TimeoutExpired) as exc:
        return UNPROVED, "runner could not complete: %r" % (exc,)

    # Parse BEFORE classifying the exit status. A checker may report a real
    # disagreement through a nonzero exit while still emitting a usable report,
    # and "checked and disagreed" must not be filed as "could not check".
    try:
        report = json.loads(p.stdout)
    except json.JSONDecodeError:
        report = None

    # An unusable report is an execution condition, not a disagreement. A list, an
    # object with no status, or a non-string status all mean the run produced nothing
    # this runner can compare -- grading that `false` would report a checked
    # disagreement that never happened.
    if report is not None and not isinstance(report, dict):
        return UNPROVED, "report is %s, not an object" % type(report).__name__
    if report is not None and not isinstance(report.get("status"), str):
        return UNPROVED, ("report carries no string `status` (got %r), so there is nothing "
                          "to compare" % (report.get("status"),))

    if report is None:
        if p.returncode != 0:
            return UNPROVED, "exit %d, no parseable report; stderr: %s" % (
                p.returncode, p.stderr.strip()[:200])
        return UNPROVED, "exit 0 but the report is not JSON"

    status = report.get("status")
    expected = suite["expect_status"]
    if status != expected:
        return FALSE, "status=%r, pinned expectation is %r (exit %d)" % (
            status, expected, p.returncode)
    if p.returncode != 0:
        # Status agrees but the checker still failed: an execution condition we
        # observed, not a disagreement we can report.
        return UNPROVED, "status matched but exit was %d; stderr: %s" % (
            p.returncode, p.stderr.strip()[:200])
    return PROVED, "status=%s %s" % (status, json.dumps(report.get("summary", {}), sort_keys=True))


def _cargo(suite: dict) -> tuple[str, str]:
    """Rust-driven corpora. Reports unproved when the toolchain is absent."""
    try:
        p = _run_capped(
            ["cargo", "test", "-p", suite["crate"], suite["cargo_target_flag"],
             suite["cargo_target"], "--", "--nocapture"],
            REPO, timeout=1800,
        )
    except FileNotFoundError:
        return UNPROVED, "cargo not on PATH"
    except _OutputTooLarge:
        return UNPROVED, "cargo output exceeded %d bytes; not materialized" % OUTPUT_CAP_BYTES
    except (OSError, subprocess.TimeoutExpired) as exc:
        return UNPROVED, "runner could not complete: %r" % (exc,)
    out = p.stdout + p.stderr
    if p.returncode == 0:
        # A filter that matches nothing also exits 0. "The suite passed" and
        # "the filter selected no tests" must not print identically, so the
        # count is read back rather than assumed.
        ran = sum(int(m) for m in re.findall(r"^test result: ok\. (\d+) passed", out, re.M))
        if ran == 0:
            return UNPROVED, ("cargo exited 0 but selected NO tests for %r -- "
                              "the target filter matches nothing" % suite["cargo_target"])
        return PROVED, "cargo test %s passed (%d tests)" % (suite["cargo_target"], ran)
    tail = out.strip().splitlines()
    hit = [ln for ln in tail if "test result:" in ln or "error[" in ln or "panicked" in ln]
    detail = " | ".join(hit[-3:]) if hit else "exit %d" % p.returncode
    # A compile/link failure is an execution condition; a failing assertion is a
    # real disagreement. Do not collapse them.
    if "error[" in out or "could not compile" in out:
        return UNPROVED, "did not build: %s" % detail[:200]
    return FALSE, detail[:200]


SUITES = [
    {
        "id": "privileged-mcp-action-v0",
        "path": "conformance/privileged-mcp-action-v0",
        "vectors": 14,
        "maturity": "frozen, digest-pinned, open reproduction request (Rul1an/assay#1840)",
        "kind": NEEDS_CANDIDATE,
        "note": "clean-room gate: score_candidate.py REQUIRES --entrypoint. "
                "No self-run by design.",
    },
    {
        "id": "mcp-jsonrpc-id-conformance",
        "path": "examples/mcp-jsonrpc-id-conformance",
        "vectors": 3,
        "maturity": "published pack, stdlib checker, carries a positive control",
        "kind": "stdlib",
        "runner": _stdlib_jsonrpc,
        "expect_status": "contradiction",
    },
    {
        "id": "rfc8785-canonicalization",
        "path": "crates/assay-canonical/tests/vectors/rfc8785.json",
        "vectors": 31,
        "maturity": "prerequisite vectors; also vendored byte-identical into the clean-room pack",
        "kind": "cargo",
        "runner": _cargo,
        "crate": "assay-canonical",
        "cargo_target_flag": "--test",
        "cargo_target": "rfc8785_conformance",
    },
    {
        "id": "mcp-era-parity-v0",
        "path": "crates/assay-core/tests/fixtures/mcp-era-parity-v0",
        "vectors": 18,
        "maturity": "EXPLORATORY -- lower than privileged-mcp-action-v0, deliberately. "
                    "No reproduction request; no claim here inherits the frozen corpus standing.",
        "kind": "cargo",
        "runner": _cargo,
        "crate": "assay-core",
        "cargo_target_flag": "--lib",
        "cargo_target": "mcp::era_parity_tests",
    },
    {
        "id": "observed-effect-v0",
        "path": "https://github.com/Rul1an/observed-effect-v0",
        "vectors": None,
        "maturity": "published in its own repository",
        "kind": EXTERNAL,
        "note": "two suites with stdlib recompute and a corpusDigest; run it from that repo.",
    },
]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--with-cargo", action="store_true",
                    help="also run the Rust-driven corpora (needs a toolchain)")
    ap.add_argument("--json", action="store_true", help="emit a machine-readable report")
    args = ap.parse_args()

    results = []
    for suite in SUITES:
        kind = suite["kind"]
        if kind in (NEEDS_CANDIDATE, EXTERNAL):
            grade, detail = kind, suite["note"]
        elif kind == "cargo" and not args.with_cargo:
            grade, detail = NOT_SELECTED, "rerun with --with-cargo"
        else:
            grade, detail = suite["runner"](suite)
        results.append({"id": suite["id"], "grade": grade, "detail": detail,
                        "vectors": suite["vectors"], "maturity": suite["maturity"]})

    ran = [r for r in results if r["grade"] in (PROVED, FALSE, UNPROVED)]
    not_run = [r for r in results if r not in ran]
    worst = max((RANK[r["grade"]] for r in results), default=0)

    if args.json:
        print(json.dumps({
            "schema": "assay.conformance.run_all.v1",
            "suites": results,
            "ran": len(ran), "not_run": len(not_run),
            "worst_grade": [PROVED, UNPROVED, FALSE][worst],
        }, indent=2, sort_keys=True))
    else:
        width = max(len(r["id"]) for r in results)
        for r in results:
            print("%-*s  %-15s  %s" % (width, r["id"], r["grade"], r["detail"]))
        print()
        print("ran: %d   did NOT run: %d   worst grade: %s"
              % (len(ran), len(not_run), [PROVED, UNPROVED, FALSE][worst]))
        if not_run:
            # State this every time. A suite that did not run is not a suite that agreed.
            print("NOT RUN (declared, not a pass): %s"
                  % ", ".join("%s=%s" % (r["id"], r["grade"]) for r in not_run))

    if any(r["grade"] == FALSE for r in results):
        return 1
    if any(r["grade"] == UNPROVED for r in results):
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
