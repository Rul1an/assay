#!/usr/bin/env python3
"""Mutation tests for the ADR-045 seal producer.

Each entry names a defect and the test that must catch it. A suite that passes
under a mutation is not testing that property, and trying is the only way to know.

This ran by hand four times in one session and went wrong four times: bites too
narrow to conclude from, and `git checkout` restores that silently wiped
uncommitted work because the fix had not been committed yet. Properties that
keep this version safe:

- the original is saved to a scratch copy and restored from *that*, never from
  git, so uncommitted work survives and no commit is needed first;
- SIGINT/SIGTERM handlers restore those scratch bytes before the process exits,
  so an interrupted push cannot leave the first mutant behind;
- a search string that does not appear is a hard failure, because a bite that
  never applied looks exactly like a bite that was caught;
- the table lives in Python rather than a shell array, because the first shell
  version shredded every entry containing a Rust closure on `|` and then lost
  three more to backslash quoting.
"""

from __future__ import annotations

import argparse
import os
import signal
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(
    subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
)
TARGET = ROOT / "crates/assay-cli/src/aee_seal.rs"

# (name, search, replacement, test that must fail)
MUTATIONS: list[tuple[str, str, str, str]] = [
    ("run-binding preimage", '"subject": env.subject_digest,', '"subject": "x",', "run_binding_matches_the_checker"),
    ("leaf domain tag", "buf.push(0x00);", "buf.push(0x01);", "observed_set_matches_the_checker"),
    ("leaf sort", "leaves.sort_unstable();", "", "the_leaf_sort_is_observable"),
    ("posture digest source", '.get("digest")', '.get("digestX")', "the_seal_carries_the_declared_posture_digest"),
    ("seal kind", 'aee_kind: "sealed".to_string(),', 'aee_kind: "arming".to_string(),', "every_carried_value_lands"),
    ("still armed", "aee_still_armed: true,", "aee_still_armed: false,", "every_carried_value_lands"),
    ("drop counts", "aee_drop_count: 0,", "aee_drop_count: 7,", "every_carried_value_lands"),
    # The standing non-claims, now assembled per vantage (#2093). Dropping the base list is the
    # mutation that matters; the proxy's extra non-claim has its own mutation below, because the two
    # fail differently -- losing the standing list understates every seal, losing the extra
    # understates only the vantage that owes it.
    ("non-claims", "let mut n = payload_non_claims();", "let mut n = Vec::new();", "every_carried_value_lands"),
    ("vantage non-claims", "n.extend(vantage.extra_non_claims());", "let _ = &vantage;", "the_proxy_vantage_declines_the_claim_the_kernel_vantage_can_make"),
    ("wire name", 'rename = "aeeKind"', 'rename = "aeeKindX"', "the_payload_member_names"),
    ("examination probe field", '"assayProbeErrno": probe.blocked_errno,', '"assayProbeErrno": "",', "the_examination_record_carries"),
    ("calendar day bound", "(1..=days_in_month).contains(&day)", "true", "a_calendar_invalid_instant"),
    ("year floor", "if year < 1 {", "if year < 0 {", "a_calendar_invalid_instant"),
    ("counted-queue loss", "*lost != 0", "*lost > 9999", "a_counted_queue_that_lost"),
    ("drop basis", 'Self::SynchronousProbe => DROP_BASIS_ASSERTED,', 'Self::SynchronousProbe => DROP_BASIS_CHECKED,', "every_carried_value_lands"),
    ("drop channel readings", 'format!("{name}={lost}")', 'format!("{name}")', "a_counted_queue_that_lost"),
    # Derived from the vantage since #2093, so the mutation moved with it. Hard-coding the kernel
    # schema is exactly the failure a second vantage introduces: a proxy seal that reports the
    # Landlock carrier as what established it.
    ("source schema derivation", "assay_source_schema: vantage.source_schema(),", 'assay_source_schema: "assay.enforcement_health.v1".to_string(),', "derived_fields_come_from_the_record"),
    ("vantage scope", "assay_seal_scope: vantage.scope(),", 'assay_seal_scope: "tcp_connect:landlock_port".to_string(),', "the_proxy_is_a_second_vantage_under_the_same_key_and_substrate"),
]

_SIGNAL_NAMES = {
    "SIGINT": signal.SIGINT,
    "SIGTERM": signal.SIGTERM,
}


def write_bytes_atomic(path: Path, data: bytes) -> None:
    """Replace path with data using same-directory os.replace (byte-safe, no git)."""
    fd, tmp_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=str(path.parent))
    tmp = Path(tmp_name)
    try:
        with os.fdopen(fd, "wb") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(tmp, path)
    except Exception:
        try:
            tmp.unlink(missing_ok=True)
        except OSError:
            pass
        raise


def check_residue(target: Path) -> list[str]:
    """Read-only: report declared mutations whose search anchors are absent.

    Applying a table entry replaces its search string once. Absence of that
    anchor means a live mutant (or a refactor that voids the table). Never
    rewrites the target.
    """
    text = target.read_text()
    findings: list[str] = []
    for name, search, _replace, _expect in MUTATIONS:
        if search not in text:
            findings.append(f"{target}: live mutant anchor missing for {name!r}")
    return findings


def run_tests() -> str:
    return subprocess.run(
        ["cargo", "test", "-q", "-p", "assay-cli", "--bin", "assay", "aee_seal"],
        capture_output=True,
        text=True,
        cwd=ROOT,
    ).stdout


def maybe_interrupt_after_mutation(name: str) -> None:
    """Test seam: deliver SIGINT/SIGTERM after a named mutant is written."""
    wanted = os.environ.get("ASSAY_AEE_SEAL_MUTATION_INTERRUPT_AFTER", "")
    if not wanted:
        return
    if wanted != name:
        return
    signame = os.environ.get("ASSAY_AEE_SEAL_MUTATION_INTERRUPT_SIGNAL", "SIGTERM")
    signum = _SIGNAL_NAMES.get(signame)
    if signum is None:
        raise SystemExit(f"unknown ASSAY_AEE_SEAL_MUTATION_INTERRUPT_SIGNAL={signame!r}")
    os.kill(os.getpid(), signum)


def run_mutations(target: Path) -> int:
    residue = check_residue(target)
    if residue:
        for line in residue:
            print(line, file=sys.stderr)
        print(
            f"refusing to mutate: {len(residue)} declared mutant anchor(s) already absent",
            file=sys.stderr,
        )
        return 1

    original = target.read_bytes()
    restored = False

    def restore() -> None:
        nonlocal restored
        if restored:
            return
        write_bytes_atomic(target, original)
        restored = True

    def on_signal(signum: int, _frame: object) -> None:
        # Restore then _exit so cleanup cannot depend on finally unwinding.
        restore()
        os._exit(128 + signum)

    # Explicit registrations — safety mutations delete these lines one at a time.
    signal.signal(signal.SIGINT, on_signal)
    signal.signal(signal.SIGTERM, on_signal)

    failures: list[str] = []
    try:
        src_text = original.decode()
        for name, search, replace, expect in MUTATIONS:
            write_bytes_atomic(target, original)
            restored = False
            if search not in src_text:
                failures.append(f"{name}: search string absent, so this mutation tested nothing")
                print(f"FAIL {name}: search string absent", file=sys.stderr)
                continue
            mutated = src_text.replace(search, replace, 1).encode()
            write_bytes_atomic(target, mutated)
            maybe_interrupt_after_mutation(name)
            out = run_tests()
            if "test result: FAILED" not in out:
                failures.append(f"{name}: suite passed under the mutation")
                print(f"FAIL {name}: suite passed under the mutation", file=sys.stderr)
            elif expect not in out:
                failures.append(f"{name}: caught, but not by {expect}")
                print(f"WARN {name}: caught, but not by {expect}", file=sys.stderr)
            else:
                print(f"ok   {name}  -> caught by {expect}")
        restore()
    except Exception:
        restore()
        raise

    if failures:
        print(f"\n{len(failures)} mutation(s) not caught by the test that names them", file=sys.stderr)
        return 1
    print(f"\nall {len(MUTATIONS)} mutations caught by the test that names them")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n", 1)[0])
    parser.add_argument(
        "--check-residue",
        action="store_true",
        help="read-only fail-closed check for declared live mutant anchors",
    )
    parser.add_argument(
        "--target",
        type=Path,
        default=TARGET,
        help="producer path to mutate or inspect (default: live aee_seal.rs)",
    )
    args = parser.parse_args(argv)

    target = args.target.resolve()
    if args.check_residue:
        findings = check_residue(target)
        if findings:
            for line in findings:
                print(line, file=sys.stderr)
            print(f"{len(findings)} declared mutant anchor(s) absent in {target}", file=sys.stderr)
            return 1
        print(f"ok   no declared mutant anchors missing in {target}")
        return 0

    return run_mutations(target)


if __name__ == "__main__":
    raise SystemExit(main())
