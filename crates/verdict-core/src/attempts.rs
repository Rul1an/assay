use crate::model::{AttemptRow, TestStatus};

#[derive(Debug, Clone)]
pub enum FailureClass {
    DeterministicFail,
    Flake,
    Error,
}

pub fn classify_attempts(attempts: &[AttemptRow]) -> FailureClass {
    use TestStatus::*;
    let mut saw_fail = false;
    let mut saw_pass = false;
    let mut saw_error = false;

    for a in attempts {
        match a.status {
            Pass => saw_pass = true,
            Fail => saw_fail = true,
            Flaky => {}
            Warn => {}
            Error => saw_error = true,
        }
    }

    if saw_error {
        FailureClass::Error
    } else if saw_fail && saw_pass {
        FailureClass::Flake
    } else {
        FailureClass::DeterministicFail
    }
}
