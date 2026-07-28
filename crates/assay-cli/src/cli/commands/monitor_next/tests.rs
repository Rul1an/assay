#![cfg(test)]

//! No tests moved in Step2 Commit A.
//! Step1 monitor contract tests remain in:
//! `crates/assay-cli/src/cli/commands/monitor.rs`.
//! See move-map/checklist artifacts in Commit C.

#[test]
fn enforcement_failure_exit_distinguishes_artifact_write_failure() {
    assert_eq!(
        super::enforcement_failure_exit(true, crate::exit_codes::EXIT_WOULD_BLOCK),
        crate::exit_codes::EXIT_WOULD_BLOCK
    );
    assert_eq!(
        super::enforcement_failure_exit(false, crate::exit_codes::EXIT_WOULD_BLOCK),
        crate::exit_codes::EXIT_INFRA_ERROR
    );
}

#[test]
fn enforcement_failure_exit_preserves_runtime_failure_and_prioritizes_carrier_failure() {
    assert_eq!(super::enforcement_failure_exit(true, 40), 40);
    assert_eq!(
        super::enforcement_failure_exit(false, 40),
        crate::exit_codes::EXIT_INFRA_ERROR
    );
    assert_eq!(
        super::enforcement_failure_exit(true, crate::exit_codes::EXIT_WOULD_BLOCK),
        crate::exit_codes::EXIT_WOULD_BLOCK
    );
}
