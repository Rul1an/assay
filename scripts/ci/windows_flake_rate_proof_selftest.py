#!/usr/bin/env python3
"""Behavioural self-test for the Windows flake-rate instrument.

Pins the classifications a rate depends on: which output counts as a pass, a
failure, a timeout or no measurement at all; how the observed rate is computed;
whether missing load is detected; and that each of those makes the run fail
closed rather than read clean. Every assertion here fails if the corresponding
branch in the instrument is inverted or dropped.

Run: `python3 scripts/ci/windows_flake_rate_proof_selftest.py`
"""

from __future__ import annotations

import sys
import time

import windows_flake_rate_proof as proof

PASS_OUTPUT = """
running 3 tests
test bounded_process::tests::kills_stdout_flood ... ok
test result: ok. 3 passed; 0 failed; 1 ignored; 0 measured; 10 filtered out; finished in 0.29s
"""

FAIL_OUTPUT = """
running 3 tests
test bounded_process::tests::kills_quiet_descendant_after_normal_parent_exit ... FAILED

---- bounded_process::tests::kills_quiet_descendant_after_normal_parent_exit stdout ----
thread 'bounded_process::tests::kills_quiet_descendant_after_normal_parent_exit' (6824) panicked at tests/support/bounded_process.rs:893:14:
normal parent completion must retain its outcome: deadline of 5s expired
test result: FAILED. 2 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 10.40s
"""

NO_TEST_OUTPUT = """
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 23 filtered out; finished in 0.00s
"""

EMPTY_OUTPUT = ""

failures: list[str] = []


def check(label: str, actual, expected) -> None:
    if actual != expected:
        failures.append(f"{label}: expected {expected!r}, got {actual!r}")


def iteration(index: int, **overrides) -> dict:
    """A measured, passing, fully loaded iteration, before any override."""
    record = {
        "index": index,
        "exit_code": 0,
        "duration_s": 6.0,
        "load": {"expected": True, "samples": 24, "alive_fraction": 1.0, "gap": False},
        **proof.parse_iteration(PASS_OUTPUT, timed_out=False),
    }
    record.update(overrides)
    return record


def check_parsing() -> None:
    passed = proof.parse_iteration(PASS_OUTPUT, timed_out=False)
    check("pass counts", (passed["passed"], passed["failed"], passed["ignored"]), (3, 0, 1))
    check("pass is measured", passed["could_not_measure"], False)
    check("pass has no failing tests", passed["failed_tests"], [])
    check("pass records no timeout", passed["timed_out"], False)

    failed = proof.parse_iteration(FAIL_OUTPUT, timed_out=False)
    check("fail counts", (failed["passed"], failed["failed"]), (2, 1))
    check(
        "fail names the test",
        failed["failed_tests"],
        ["bounded_process::tests::kills_quiet_descendant_after_normal_parent_exit"],
    )
    check("fail is measured", failed["could_not_measure"], False)
    check("fail captures one panic", len(failed["panics"]), 1)
    check(
        "panic message is captured",
        failed["panics"][0]["message"].startswith("normal parent completion must retain"),
        True,
    )

    # A filter that matched nothing exits 0 and looks perfect. It measured
    # nothing, and treating it as a clean run is how this instrument would lie.
    nothing = proof.parse_iteration(NO_TEST_OUTPUT, timed_out=False)
    check("zero executed tests measured nothing", nothing["could_not_measure"], True)
    check("no summary at all measured nothing", proof.parse_iteration(EMPTY_OUTPUT, False)["could_not_measure"], True)

    # A timeout did measure something: the selection did not finish in its
    # ceiling. It must stay data rather than become an instrument failure.
    hung = proof.parse_iteration(EMPTY_OUTPUT, timed_out=True)
    check("timeout is measured", hung["could_not_measure"], False)
    check("timeout is recorded", hung["timed_out"], True)


def check_rate() -> None:
    clean = [iteration(i) for i in (1, 2, 3, 4)]
    check("all-pass rate", proof.summarize(clean, 4)["observed_failure_rate"], 0.0)

    mixed = [
        iteration(1),
        iteration(2, exit_code=101, **proof.parse_iteration(FAIL_OUTPUT, timed_out=False)),
        iteration(3),
        iteration(4),
    ]
    summary = proof.summarize(mixed, 4)
    check("one failure in four", summary["observed_failure_rate"], 0.25)
    check("failed runs counted", summary["failed_runs"], 1)
    check(
        "failure attributed to the test",
        summary["failures_by_test"],
        {"bounded_process::tests::kills_quiet_descendant_after_normal_parent_exit": 1},
    )

    # A timed-out iteration is a failed run: the selection did not pass.
    timed_out = [iteration(1), iteration(2, exit_code=None, **proof.parse_iteration(EMPTY_OUTPUT, True))]
    summary = proof.summarize(timed_out, 2)
    check("timeout counts as a failure", summary["failed_runs"], 1)
    check("timeout is listed", summary["timed_out"], [2])
    check("timeout rate", summary["observed_failure_rate"], 0.5)

    # An iteration that measured nothing is excluded from the denominator
    # instead of diluting the rate.
    unmeasured = [iteration(1), iteration(2, **proof.parse_iteration(NO_TEST_OUTPUT, False))]
    summary = proof.summarize(unmeasured, 2)
    check("unmeasured excluded from denominator", summary["measured"], 1)
    check("unmeasured listed", summary["could_not_measure"], [2])


def check_fail_closed() -> None:
    clean = proof.summarize([iteration(i) for i in (1, 2)], 2)
    check("a clean measurement passes", proof.instrument_verdict(clean, 2), (0, ""))

    # Fewer iterations than requested is not a smaller measurement, it is an
    # incomplete one.
    check("short run fails", proof.instrument_verdict(clean, 1)[0], proof.EXIT_COULD_NOT_MEASURE)

    unmeasured = proof.summarize(
        [iteration(1), iteration(2, **proof.parse_iteration(NO_TEST_OUTPUT, False))], 2
    )
    check(
        "no-test iteration fails the run",
        proof.instrument_verdict(unmeasured, 2)[0],
        proof.EXIT_COULD_NOT_MEASURE,
    )

    # The load arm's own failure mode: a green result that means "no load"
    # rather than "no defect".
    gapped = proof.summarize(
        [
            iteration(1),
            iteration(2, load={"expected": True, "samples": 20, "alive_fraction": 0.4, "gap": True}),
        ],
        2,
    )
    check("absent load fails the run", proof.instrument_verdict(gapped, 2)[0], proof.EXIT_LOAD_ABSENT)
    check("absent load names the iteration", gapped["load_gaps"], [2])


def check_load_supervision() -> None:
    quiet = proof.LoadSupervisor(None)
    quiet.start()
    quiet.begin_iteration()
    coverage = quiet.iteration_coverage()
    check("no load arm expects none", coverage["expected"], False)
    check("no load arm reports no gap", coverage["gap"], False)
    quiet.stop()

    # A load command that exits at once cannot cover an iteration, however
    # eagerly it is restarted. Without this the arm silently measures no load.
    dying = proof.LoadSupervisor(
        [sys.executable, "-c", "pass"], interval_s=0.05
    )
    dying.start()
    dying.begin_iteration()
    time.sleep(1.0)
    coverage = dying.iteration_coverage()
    dying.stop()
    check("a load that keeps dying is a gap", coverage["gap"], True)
    check("restarts are visible", dying.starts > 1, True)

    alive = proof.LoadSupervisor(
        [sys.executable, "-c", "import time; time.sleep(30)"], interval_s=0.05
    )
    alive.start()
    alive.begin_iteration()
    time.sleep(1.0)
    coverage = alive.iteration_coverage()
    alive.stop()
    check("a live load is no gap", coverage["gap"], False)
    check("a live load is fully covered", coverage["alive_fraction"], 1.0)
    check("a live load starts once", alive.starts, 1)


def check_claim_surface() -> None:
    # The manifest states its ceiling and its reuse rule; a proof that carries
    # further than its author intended does so through this text.
    check("reuse is pinned to the commit", "head_sha" in proof.REUSE_RULE, True)
    check(
        "the ceiling claims no verdict",
        proof.CLAIM_CEILING,
        "hosted_windows_flake_rate_measurement_only_not_a_verdict",
    )
    check(
        "causation is disclaimed in the pack",
        any("no causal claim" in claim for claim in proof.NON_CLAIMS),
        True,
    )


def main() -> int:
    check_parsing()
    check_rate()
    check_fail_closed()
    check_load_supervision()
    check_claim_surface()
    for failure in failures:
        print(f"FAIL {failure}", file=sys.stderr)
    if failures:
        print(f"{len(failures)} self-test assertion(s) failed", file=sys.stderr)
        return 1
    print("windows flake-rate instrument self-test: all assertions hold")
    return 0


if __name__ == "__main__":
    sys.exit(main())
