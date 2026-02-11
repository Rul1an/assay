use crate::attacks;
use crate::differential;
use crate::report::{AttackResult, AttackStatus, SimReport};
use anyhow::Result;
use assay_evidence::VerifyLimits;
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
    pub target_bundle: PathBuf,
    pub seed: u64,
    pub verify_limits: Option<VerifyLimits>,
    /// Time budget in seconds (default 60). Used to create TimeBudget.
    pub time_budget_secs: u64,
}

/// Time budget for an entire suite run.
///
/// If the elapsed time exceeds the budget, remaining phases are skipped and
/// the runner reports `AttackStatus::Error` with "time budget exceeded".
#[derive(Debug, Clone)]
pub struct TimeBudget {
    start: Instant,
    limit: Duration,
}

/// Tier-specific default limits (ADR-024: Quick 5MB to keep suite fast).
/// Single source of truth for tier defaults; used by CLI and suite.
/// Input is normalized (trim + lowercase) for case-insensitive matching.
pub fn tier_default_limits(tier: &str) -> VerifyLimits {
    let mut defaults = VerifyLimits::default();
    if tier.trim().to_lowercase() == "quick" {
        defaults.max_bundle_bytes = 5 * 1024 * 1024; // 5 MB
    }
    defaults
}

impl TimeBudget {
    pub fn new(limit: Duration) -> Self {
        Self {
            start: Instant::now(),
            limit,
        }
    }

    /// Default suite budget: 60 seconds.
    /// Note: Raised from 30s because zip bomb attack (1.1GB decompression)
    /// can take 30+ seconds on slower CI runners (macOS).
    pub fn default_suite() -> Self {
        Self::new(Duration::from_secs(60))
    }

    pub fn exceeded(&self) -> bool {
        self.start.elapsed() > self.limit
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    pub fn remaining(&self) -> Duration {
        self.limit.saturating_sub(self.start.elapsed())
    }
}

pub fn run_suite(cfg: SuiteConfig) -> Result<SimReport> {
    let mut report = SimReport::new(&format!("{:?}", cfg.tier), cfg.seed);
    let budget = TimeBudget::new(Duration::from_secs(cfg.time_budget_secs));
    let limits = cfg
        .verify_limits
        .unwrap_or_else(|| tier_default_limits(&format!("{:?}", cfg.tier).to_lowercase()));

    // 1. Integrity Attacks (all tiers)
    //
    // Note: The workspace uses panic="abort" in dev/release profiles, so catch_unwind
    // is not effective. Integrity attacks run in-process (they don't trigger panics —
    // they test verification outcomes). Chaos/differential attacks use subprocess
    // isolation instead.
    {
        let seed = cfg.seed;
        let start = Instant::now();
        let mut inner_report = SimReport::new("integrity", seed);
        match attacks::integrity::check_integrity_attacks(&mut inner_report, seed, limits, &budget)
        {
            Ok(()) => {
                for r in inner_report.results {
                    report.add_result(r);
                }
            }
            Err(attacks::integrity::IntegrityError::BudgetExceeded) => {
                for r in inner_report.results {
                    report.add_result(r);
                }
                report.set_time_budget_exceeded(vec!["differential".into(), "chaos".into()]);
                report.add_result(AttackResult {
                    name: "integrity.time_budget".into(),
                    status: AttackStatus::Error,
                    error_class: None,
                    error_code: None,
                    message: Some("time budget exceeded during integrity phase".into()),
                    duration_ms: budget.elapsed().as_millis() as u64,
                });
                return Ok(report);
            }
            Err(attacks::integrity::IntegrityError::Other(e)) => {
                for r in inner_report.results {
                    report.add_result(r);
                }
                report.add_result(AttackResult {
                    name: "integrity_attacks".into(),
                    status: AttackStatus::Error,
                    error_class: None,
                    error_code: None,
                    message: Some(e.to_string()),
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
        }
    }

    if budget.exceeded() {
        report.set_time_budget_exceeded(vec!["differential".into(), "chaos".into()]);
        report.add_result(AttackResult {
            name: "integrity.time_budget".into(),
            status: AttackStatus::Error,
            error_class: None,
            error_code: None,
            message: Some("time budget exceeded after integrity phase".into()),
            duration_ms: budget.elapsed().as_millis() as u64,
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
        let inner = differential::check_invariants(iterations, Some(cfg.seed));
        let duration = start.elapsed().as_millis() as u64;
        report.add_check("differential.invariants", inner, duration);
    }

    if budget.exceeded() {
        report.set_time_budget_exceeded(vec!["chaos".into()]);
        report.add_result(AttackResult {
            name: "differential.time_budget".into(),
            status: AttackStatus::Error,
            error_class: None,
            error_code: None,
            message: Some("time budget exceeded after differential phase".into()),
            duration_ms: budget.elapsed().as_millis() as u64,
        });
        return Ok(report);
    }

    // 3. Chaos-tier extras (use subprocess isolation for panic=abort safety)
    if matches!(cfg.tier, SuiteTier::Chaos) {
        run_chaos_phase(&mut report, cfg.seed, &budget);
    }

    Ok(report)
}

fn run_chaos_phase(report: &mut SimReport, seed: u64, budget: &TimeBudget) {
    // Fail-fast: skip chaos if already over budget
    if budget.exceeded() {
        report.set_time_budget_exceeded(vec![]);
        report.add_result(AttackResult {
            name: "chaos.time_budget".into(),
            status: AttackStatus::Error,
            error_class: None,
            error_code: None,
            message: Some("time budget exceeded before chaos phase".into()),
            duration_ms: budget.elapsed().as_millis() as u64,
        });
        report.add_result(AttackResult {
            name: "differential.parity".into(),
            status: AttackStatus::Error,
            error_class: None,
            error_code: None,
            message: Some("skipped due to time budget".into()),
            duration_ms: 0,
        });
        return;
    }

    // IO chaos attacks (in-process — these inject IO errors, not panics)
    match attacks::chaos::check_chaos_attacks(seed) {
        Ok(results) => {
            for r in results {
                report.add_result(r);
            }
        }
        Err(e) => {
            report.add_result(AttackResult {
                name: "chaos.io_faults".into(),
                status: AttackStatus::Error,
                error_class: None,
                error_code: None,
                message: Some(format!("chaos attacks failed: {}", e)),
                duration_ms: 0,
            });
        }
    }

    if budget.exceeded() {
        report.set_time_budget_exceeded(vec![]);
        report.add_result(AttackResult {
            name: "chaos.time_budget".into(),
            status: AttackStatus::Error,
            error_class: None,
            error_code: None,
            message: Some("time budget exceeded during chaos phase".into()),
            duration_ms: budget.elapsed().as_millis() as u64,
        });
        // Optie C: make skipped work visible (parity was not run)
        report.add_result(AttackResult {
            name: "differential.parity".into(),
            status: AttackStatus::Error,
            error_class: None,
            error_code: None,
            message: Some("skipped due to time budget".into()),
            duration_ms: 0,
        });
        return;
    }

    // Differential parity checks (uses subprocess isolation for production verifier)
    match attacks::differential::check_differential_parity(seed) {
        Ok(results) => {
            for r in results {
                report.add_result(r);
            }
        }
        Err(e) => {
            report.add_result(AttackResult {
                name: "differential.parity".into(),
                status: AttackStatus::Error,
                error_class: None,
                error_code: None,
                message: Some(format!("differential parity failed: {}", e)),
                duration_ms: 0,
            });
        }
    }
}
