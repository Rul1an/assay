use crate::model::{AttemptRow, TestStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    DeterministicPass,
    DeterministicFail,
    Flaky,    // fail -> pass
    Unstable, // pass <-> fail patterns (non-monotonic)
    Error,    // any error (or deterministic error)
    Skipped,
}

pub fn classify_attempts(attempts: &[AttemptRow]) -> FailureClass {
    use TestStatus::*;

    if attempts.is_empty() {
        return FailureClass::Error;
    }

    if attempts.len() == 1 {
        if let Skipped = attempts[0].status {
            return FailureClass::Skipped;
        }
    }

    let has_error = attempts.iter().any(|a| matches!(a.status, Error));
    if has_error {
        return FailureClass::Error;
    }

    let statuses: Vec<TestStatus> = attempts.iter().map(|a| a.status.clone()).collect();

    let any_fail = statuses.iter().any(|s| matches!(s, Fail));
    let any_pass = statuses.iter().any(|s| matches!(s, Pass | Warn | Flaky));
    // Note: Treats Warn/Flaky as form of pass for "did it work eventually?" perspective
    // or strictly Pass? Plan says "Pass: all pass".
    // But for Flaky detection: Fail -> Pass.
    // Let's stick to explicit Match types.

    if any_fail && any_pass {
        // Detect classic flaky: Fail... then Pass (and stays Pass?)
        // User definition: "fail->pass".
        // Unstable: "mixed".

        let first_fail_idx = statuses.iter().position(|s| matches!(s, Fail));
        let first_pass_idx = statuses.iter().position(|s| matches!(s, Pass)); // Strictly Pass?

        if let (Some(fail_i), Some(pass_i)) = (first_fail_idx, first_pass_idx) {
            if fail_i < pass_i {
                // Check if it stays passing?
                // User snippet: "Fail then later Pass, and last is Pass"
                let last = statuses.last().unwrap();
                if matches!(last, Pass) {
                    return FailureClass::Flaky;
                }
            }
        }
        return FailureClass::Unstable;
    }

    if any_fail {
        return FailureClass::DeterministicFail;
    }

    // If we are here, no errors, no fails.
    FailureClass::DeterministicPass
}
