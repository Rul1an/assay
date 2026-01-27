use anyhow::Result;
use assay_evidence::bundle::writer::{ErrorClass, ErrorCode};
use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct SimReport {
    pub suite: String,
    pub seed: u64,
    pub summary: SimSummary,
    pub results: Vec<AttackResult>,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct SimSummary {
    pub total: usize,
    pub passed: usize,   // New: For invariants
    pub blocked: usize,  // For attacks
    pub bypassed: usize, // For attacks
    pub failed: usize,   // New: For invariants
    pub errors: usize,
}

#[derive(Debug, Serialize, Clone)]
pub struct AttackResult {
    pub name: String,
    pub status: AttackStatus,
    pub error_class: Option<String>,
    pub error_code: Option<String>,
    pub message: Option<String>,
    pub duration_ms: u64, // New: DX requirement
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub enum AttackStatus {
    Passed,   // New: Invariant held
    Failed,   // New: Invariant broken
    Blocked,  // Attack was stopped
    Bypassed, // Attack succeeded
    Error,    // Infrastructure error
}

impl SimReport {
    pub fn new(suite: &str, seed: u64) -> Self {
        Self {
            suite: suite.to_string(),
            seed,
            summary: SimSummary::default(),
            results: Vec::new(),
        }
    }

    pub fn add_attack(
        &mut self,
        name: &str,
        result: Result<(ErrorClass, ErrorCode), anyhow::Error>,
        duration_ms: u64,
    ) {
        self.summary.total += 1;
        let res = match result {
            Ok((class, code)) => {
                self.summary.blocked += 1;
                AttackResult {
                    name: name.to_string(),
                    status: AttackStatus::Blocked,
                    error_class: Some(format!("{:?}", class)),
                    error_code: Some(format!("{:?}", code)),
                    message: None,
                    duration_ms,
                }
            }
            Err(e) => {
                self.summary.bypassed += 1;
                AttackResult {
                    name: name.to_string(),
                    status: AttackStatus::Bypassed,
                    error_class: None,
                    error_code: None,
                    message: Some(e.to_string()),
                    duration_ms,
                }
            }
        };
        self.results.push(res);
    }

    /// Add a pre-built AttackResult directly.
    pub fn add_result(&mut self, result: AttackResult) {
        self.summary.total += 1;
        match result.status {
            AttackStatus::Passed => self.summary.passed += 1,
            AttackStatus::Failed => self.summary.failed += 1,
            AttackStatus::Blocked => self.summary.blocked += 1,
            AttackStatus::Bypassed => self.summary.bypassed += 1,
            AttackStatus::Error => self.summary.errors += 1,
        }
        self.results.push(result);
    }

    pub fn add_check(&mut self, name: &str, result: Result<()>, duration_ms: u64) {
        self.summary.total += 1;
        let res = match result {
            Ok(_) => {
                self.summary.passed += 1;
                AttackResult {
                    name: name.to_string(),
                    status: AttackStatus::Passed,
                    error_class: None,
                    error_code: None,
                    message: None,
                    duration_ms,
                }
            }
            Err(e) => {
                self.summary.failed += 1;
                AttackResult {
                    name: name.to_string(),
                    status: AttackStatus::Failed,
                    error_class: None,
                    error_code: None,
                    message: Some(e.to_string()),
                    duration_ms,
                }
            }
        };
        self.results.push(res);
    }
}
