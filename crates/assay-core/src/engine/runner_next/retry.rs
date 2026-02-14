use crate::attempts::FailureClass;
use crate::model::{AttemptRow, TestCase, TestResultRow, TestStatus};

pub(crate) fn record_attempt_impl(
    attempts: &mut Vec<AttemptRow>,
    attempt_no: u32,
    row: &TestResultRow,
) {
    attempts.push(AttemptRow {
        attempt_no,
        status: row.status,
        message: row.message.clone(),
        duration_ms: row.duration_ms,
        details: row.details.clone(),
    });
}

pub(crate) fn should_stop_retries_impl(status: TestStatus) -> bool {
    matches!(
        status,
        TestStatus::Pass | TestStatus::Warn | TestStatus::AllowedOnError | TestStatus::Skipped
    )
}

pub(crate) fn no_attempts_row_impl(tc: &TestCase) -> TestResultRow {
    TestResultRow {
        test_id: tc.id.clone(),
        status: TestStatus::Error,
        score: None,
        cached: false,
        message: "no attempts".into(),
        details: serde_json::json!({}),
        duration_ms: None,
        fingerprint: None,
        skip_reason: None,
        attempts: None,
        error_policy_applied: None,
    }
}

pub(crate) fn apply_failure_classification_impl(
    final_row: &mut TestResultRow,
    class: FailureClass,
    attempt_len: usize,
) {
    match class {
        FailureClass::Skipped => {
            final_row.status = TestStatus::Skipped;
            // message usually set by run_test_once
        }
        FailureClass::Flaky => {
            final_row.status = TestStatus::Flaky;
            final_row.message = "flake detected (rerun passed)".into();
            final_row.details["flake"] = serde_json::json!({ "attempts": attempt_len });
        }
        FailureClass::Unstable => {
            final_row.status = TestStatus::Unstable;
            final_row.message = "unstable outcomes detected".into();
            final_row.details["unstable"] = serde_json::json!({ "attempts": attempt_len });
        }
        FailureClass::Error => final_row.status = TestStatus::Error,
        FailureClass::DeterministicFail => {
            final_row.status = TestStatus::Fail;
        }
        FailureClass::DeterministicPass => {
            if final_row.status != TestStatus::AllowedOnError {
                final_row.status = TestStatus::Pass;
            }
        }
    }
}
