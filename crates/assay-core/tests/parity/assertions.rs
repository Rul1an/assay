use crate::{CheckInput, CheckResult, Outcome, PolicyCheck};

/// Compute a canonical hash for result comparison
pub fn compute_result_hash(check_id: &str, outcome: &Outcome, reason: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    check_id.hash(&mut hasher);
    format!("{:?}", outcome).hash(&mut hasher);
    reason.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Compare batch and streaming results
pub fn verify_parity(check: &PolicyCheck, input: &CheckInput) -> ParityResult {
    let batch_result = crate::batch::evaluate(check, input);
    let streaming_result = crate::streaming::evaluate(check, input);

    let is_identical = batch_result.outcome == streaming_result.outcome
        && batch_result.reason == streaming_result.reason;

    ParityResult {
        check_id: check.id.clone(),
        batch_result,
        streaming_result,
        is_identical,
    }
}

#[derive(Debug)]
pub struct ParityResult {
    pub check_id: String,
    pub batch_result: CheckResult,
    pub streaming_result: CheckResult,
    pub is_identical: bool,
}

impl ParityResult {
    pub fn assert_parity(&self) {
        if !self.is_identical {
            panic!(
                "PARITY VIOLATION for check '{}':\n\
                 Batch:     {:?} - {}\n\
                 Streaming: {:?} - {}\n\
                 \n\
                 This is a CRITICAL bug. Batch and streaming modes must produce identical results.",
                self.check_id,
                self.batch_result.outcome,
                self.batch_result.reason,
                self.streaming_result.outcome,
                self.streaming_result.reason,
            );
        }
    }
}
