#![cfg(test)]

//! No tests moved in Step2 Commit A.
//! Step1 monitor contract tests remain in:
//! `crates/assay-cli/src/cli/commands/monitor.rs`.
//! See move-map/checklist artifacts in Commit C.

#[test]
fn enforcement_refusal_exit_distinguishes_artifact_write_failure() {
    assert_eq!(
        super::enforcement_refusal_exit(true),
        crate::exit_codes::EXIT_WOULD_BLOCK
    );
    assert_eq!(
        super::enforcement_refusal_exit(false),
        crate::exit_codes::EXIT_INFRA_ERROR
    );
}
