use crate::attacks;
use crate::differential;
use crate::report::{AttackResult, AttackStatus, SimReport};
use anyhow::Result;
use assay_evidence::VerifyLimits;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub enum SuiteTier {
    Quick,
    Nightly,
    Stress,
    Chaos,
}

#[derive(Debug, Clone)]
pub struct SuiteConfig {
    pub tier: SuiteTier,
    pub target_bundle: PathBuf, // Placeholder for future file-based targets
    pub seed: u64,
    pub verify_limits: Option<VerifyLimits>,
}

/// Time budget for a single attack or check.
///
/// If the elapsed time exceeds the budget, the runner reports
/// `AttackStatus::Error` with "time budget exceeded".
#[derive(Debug, Clone)]
pub struct TimeBudget {
    start: Instant,
    limit: Duration,
}

impl TimeBudget {
    pub fn new(limit: Duration) -> Self {
        Self {
            start: Instant::now(),
            limit,
        }
    }

    /// Default budget: 30 seconds per attack.
    pub fn default_per_attack() -> Self {
        Self::new(Duration::from_secs(30))
    }

    pub fn exceeded(&self) -> bool {
        self.start.elapsed() > self.limit
    }

    pub fn remaining(&self) -> Duration {
        self.limit.saturating_sub(self.start.elapsed())
    }
}

pub fn run_suite(cfg: SuiteConfig) -> Result<SimReport> {
    let mut report = SimReport::new(&format!("{:?}", cfg.tier), cfg.seed);
    let budget = TimeBudget::default_per_attack();

    // 1. Integrity Attacks (all tiers) — catch_unwind shielded
    {
        let seed = cfg.seed;
        let start = Instant::now();
        let res = panic::catch_unwind(AssertUnwindSafe(|| {
            let mut inner_report = SimReport::new("integrity", seed);
            let r = attacks::integrity::check_integrity_attacks(&mut inner_report, seed);
            (inner_report, r)
        }));
        let duration = start.elapsed().as_millis() as u64;

        match res {
            Ok((inner_report, Ok(()))) => {
                for r in inner_report.results {
                    report.add_result(r);
                }
            }
            Ok((inner_report, Err(e))) => {
                // Collect partial results before error
                for r in inner_report.results {
                    report.add_result(r);
                }
                report.add_result(AttackResult {
                    name: "integrity_attacks".into(),
                    status: AttackStatus::Error,
                    error_class: None,
                    error_code: None,
                    message: Some(e.to_string()),
                    duration_ms: duration,
                });
            }
            Err(panic_info) => {
                let msg = panic_message(&panic_info);
                report.add_result(AttackResult {
                    name: "integrity_attacks".into(),
                    status: AttackStatus::Error,
                    error_class: None,
                    error_code: None,
                    message: Some(format!("panic: {}", msg)),
                    duration_ms: duration,
                });
            }
        }
    }

    if budget.exceeded() {
        report.add_result(AttackResult {
            name: "time_budget".into(),
            status: AttackStatus::Error,
            error_class: None,
            error_code: None,
            message: Some("time budget exceeded after integrity attacks".into()),
            duration_ms: budget.start.elapsed().as_millis() as u64,
        });
        return Ok(report);
    }

    // 2. Differential Testing
    let iterations = match cfg.tier {
        SuiteTier::Quick => 5,
        SuiteTier::Nightly => 100,
        SuiteTier::Stress => 1000,
        SuiteTier::Chaos => 50,
    };

    {
        let start = Instant::now();
        let res = panic::catch_unwind(AssertUnwindSafe(|| {
            differential::check_invariants(iterations, Some(cfg.seed))
        }));
        let duration = start.elapsed().as_millis() as u64;

        match res {
            Ok(inner) => report.add_check("differential.invariants", inner, duration),
            Err(panic_info) => {
                let msg = panic_message(&panic_info);
                report.add_result(AttackResult {
                    name: "differential.invariants".into(),
                    status: AttackStatus::Error,
                    error_class: None,
                    error_code: None,
                    message: Some(format!("panic: {}", msg)),
                    duration_ms: duration,
                });
            }
        }
    }

    if budget.exceeded() {
        report.add_result(AttackResult {
            name: "time_budget".into(),
            status: AttackStatus::Error,
            error_class: None,
            error_code: None,
            message: Some("time budget exceeded after differential tests".into()),
            duration_ms: budget.start.elapsed().as_millis() as u64,
        });
        return Ok(report);
    }

    // 3. Chaos-tier extras
    if matches!(cfg.tier, SuiteTier::Chaos) {
        run_chaos_phase(&mut report, cfg.seed, &budget);
    }

    Ok(report)
}

fn run_chaos_phase(report: &mut SimReport, seed: u64, budget: &TimeBudget) {
    // IO chaos attacks
    {
        let start = Instant::now();
        let res = panic::catch_unwind(AssertUnwindSafe(|| {
            attacks::chaos::check_chaos_attacks(seed)
        }));
        let duration = start.elapsed().as_millis() as u64;

        match res {
            Ok(Ok(results)) => {
                for r in results {
                    report.add_result(r);
                }
            }
            Ok(Err(e)) => {
                report.add_result(AttackResult {
                    name: "chaos.io_faults".into(),
                    status: AttackStatus::Error,
                    error_class: None,
                    error_code: None,
                    message: Some(format!("chaos attacks failed: {}", e)),
                    duration_ms: duration,
                });
            }
            Err(panic_info) => {
                let msg = panic_message(&panic_info);
                report.add_result(AttackResult {
                    name: "chaos.io_faults".into(),
                    status: AttackStatus::Error,
                    error_class: None,
                    error_code: None,
                    message: Some(format!("panic: {}", msg)),
                    duration_ms: duration,
                });
            }
        }
    }

    if budget.exceeded() {
        report.add_result(AttackResult {
            name: "time_budget".into(),
            status: AttackStatus::Error,
            error_class: None,
            error_code: None,
            message: Some("time budget exceeded during chaos phase".into()),
            duration_ms: 0,
        });
        return;
    }

    // Differential parity checks
    {
        let start = Instant::now();
        let res = panic::catch_unwind(AssertUnwindSafe(|| {
            attacks::differential::check_differential_parity(seed)
        }));
        let duration = start.elapsed().as_millis() as u64;

        match res {
            Ok(Ok(results)) => {
                for r in results {
                    report.add_result(r);
                }
            }
            Ok(Err(e)) => {
                report.add_result(AttackResult {
                    name: "differential.parity".into(),
                    status: AttackStatus::Error,
                    error_class: None,
                    error_code: None,
                    message: Some(format!("differential parity failed: {}", e)),
                    duration_ms: duration,
                });
            }
            Err(panic_info) => {
                let msg = panic_message(&panic_info);
                report.add_result(AttackResult {
                    name: "differential.parity".into(),
                    status: AttackStatus::Error,
                    error_class: None,
                    error_code: None,
                    message: Some(format!("panic: {}", msg)),
                    duration_ms: duration,
                });
            }
        }
    }
}

/// Extract a human-readable message from a panic payload.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".into()
    }
}
