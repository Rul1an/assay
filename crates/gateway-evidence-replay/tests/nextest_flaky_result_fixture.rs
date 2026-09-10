//! Fixtures for `scripts/ci/test-nextest-flaky-result.sh` (#2871).
//!
//! `.config/nextest.toml` sets `flaky-result = "fail"`, so a test that fails once and passes on
//! retry is reported as a failure rather than silently as a pass. Proving that needs a test which
//! is flaky by construction, and a clean test beside it to show the run does not label everything.
//!
//! Nextest runs each attempt in a fresh process and sets `NEXTEST_ATTEMPT` in it, so the attempt
//! number is the whole mechanism: no marker file, no scratch directory, no path built from the
//! environment. An earlier version used a marker file whose path came from an environment
//! variable; CodeQL correctly flagged that as path injection (`rust/path-injection`, high), and
//! this shape has no path to inject into.
//!
//! Both tests are `#[ignore]`, so no ordinary suite runs them: `cargo test --workspace` in the
//! required CI job lists them as ignored and never executes them. The script selects them by name
//! with `--run-ignored ignored-only`.
//!
//! They live in this crate because the fixture has to sit somewhere nextest runs, and this is a
//! `publish = false` workspace leaf outside the delegated-runner gated paths. `assay-xtask` is
//! where build tooling belongs semantically, but `crates/assay-xtask/` is in `all_gate_prefixes`,
//! so a fixture there would make every change to it require a delegated runner proof.

/// Fails on the first attempt and passes on the retry, which is exactly the shape
/// `retries = 1` used to report as a clean pass.
#[test]
#[ignore = "flaky by construction; run only through scripts/ci/test-nextest-flaky-result.sh"]
fn flaky_by_construction_fails_once_then_passes() {
    let attempt = nextest_attempt();
    assert!(
        attempt > 1,
        "attempt {attempt} fails by construction; the retry must pass"
    );
}

/// Passes on its first attempt. The script asserts this one carries no flake label, so a profile
/// that labelled every test a flake would not satisfy the contract either.
#[test]
#[ignore = "control for scripts/ci/test-nextest-flaky-result.sh"]
fn clean_by_construction_passes_first_attempt() {
    assert_eq!(
        nextest_attempt(),
        1,
        "the clean control must not be retried; it passes on its first attempt"
    );
}

/// The attempt number nextest set for this process, 1-based.
fn nextest_attempt() -> u32 {
    let raw = std::env::var("NEXTEST_ATTEMPT").expect(
        "NEXTEST_ATTEMPT is unset: these fixtures are driven by cargo nextest, \
         through scripts/ci/test-nextest-flaky-result.sh",
    );
    raw.parse()
        .unwrap_or_else(|_| panic!("NEXTEST_ATTEMPT must be an integer, got {raw:?}"))
}
