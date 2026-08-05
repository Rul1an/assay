#!/usr/bin/env python3
"""Mutation tests for the ADR-045 seal producer.

Each entry names a defect and the test that must catch it. A suite that passes
under a mutation is not testing that property, and trying is the only way to know.

This ran by hand four times in one session and went wrong four times: bites too
narrow to conclude from, and `git checkout` restores that silently wiped
uncommitted work because the fix had not been committed yet. Three properties
make this version safe:

- the original is saved to a scratch copy and restored from *that*, never from
  git, so uncommitted work survives and no commit is needed first;
- a search string that does not appear is a hard failure, because a bite that
  never applied looks exactly like a bite that was caught;
- the table lives in Python rather than a shell array, because the first shell
  version shredded every entry containing a Rust closure on `|` and then lost
  three more to backslash quoting.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True).stdout.strip())
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
    ("non-claims", "assay_non_claims: payload_non_claims(),", "assay_non_claims: Vec::new(),", "every_carried_value_lands"),
    ("wire name", 'rename = "aeeKind"', 'rename = "aeeKindX"', "the_payload_member_names"),
    ("examination probe field", '"assayProbeErrno": probe.blocked_errno,', '"assayProbeErrno": "",', "the_examination_record_carries"),
    ("calendar day bound", "(1..=days_in_month).contains(&day)", "true", "a_calendar_invalid_instant"),
    ("year floor", "if year < 1 {", "if year < 0 {", "a_calendar_invalid_instant"),
    ("counted-queue loss", "*lost != 0", "*lost > 9999", "a_counted_queue_that_lost"),
    ("source schema derivation", "assay_source_schema: health.schema.clone(),", 'assay_source_schema: "assay.enforcement_health.v1".to_string(),', "derived_fields_come_from_the_record"),
]


def run_tests() -> str:
    return subprocess.run(
        ["cargo", "test", "-q", "-p", "assay-cli", "--bin", "assay", "aee_seal"],
        capture_output=True, text=True, cwd=ROOT,
    ).stdout


def main() -> int:
    scratch = Path(tempfile.mkdtemp())
    original = scratch / "original.rs"
    shutil.copy(TARGET, original)
    failures: list[str] = []
    try:
        for name, search, replace, expect in MUTATIONS:
            shutil.copy(original, TARGET)
            src = original.read_text()
            if search not in src:
                failures.append(f"{name}: search string absent, so this mutation tested nothing")
                print(f"FAIL {name}: search string absent", file=sys.stderr)
                continue
            TARGET.write_text(src.replace(search, replace, 1))
            out = run_tests()
            if "test result: FAILED" not in out:
                failures.append(f"{name}: suite passed under the mutation")
                print(f"FAIL {name}: suite passed under the mutation", file=sys.stderr)
            elif expect not in out:
                failures.append(f"{name}: caught, but not by {expect}")
                print(f"WARN {name}: caught, but not by {expect}", file=sys.stderr)
            else:
                print(f"ok   {name}  -> caught by {expect}")
    finally:
        shutil.copy(original, TARGET)
        shutil.rmtree(scratch, ignore_errors=True)

    if failures:
        print(f"\n{len(failures)} mutation(s) not caught by the test that names them", file=sys.stderr)
        return 1
    print(f"\nall {len(MUTATIONS)} mutations caught by the test that names them")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
