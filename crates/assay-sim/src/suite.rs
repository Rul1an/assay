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

/// The phases a tier never runs.
///
/// Kept beside `run_suite`'s phase gates rather than derived from them, because the
/// gates are `if` statements over a tier and cannot be enumerated. The pairing is
/// pinned by `chaos_phase_gate_matches_the_declared_omission`, so a new gate that
/// forgets this list fails a test instead of quietly overstating the run.
fn phases_not_attempted_for(tier: &SuiteTier) -> Vec<String> {
    match tier {
        SuiteTier::Chaos => Vec::new(),
        SuiteTier::Quick | SuiteTier::Nightly | SuiteTier::Stress => vec!["chaos".to_string()],
    }
}

pub fn run_suite(cfg: SuiteConfig) -> Result<SimReport> {
    let mut report = SimReport::new(&format!("{:?}", cfg.tier), cfg.seed);
    // The chaos phase is gated on the Chaos tier below. Naming it here keeps the
    // statement next to the gate that causes it, so a new gate that forgets to
    // update this list is a visible omission rather than a silent one.
    report.set_phases_not_attempted(phases_not_attempted_for(&cfg.tier));
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
        report.set_time_budget_exceeded(vec!["chaos".into()]);
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

#[cfg(test)]
mod not_attempted_tests {
    use super::*;

    /// The tiers that skip the chaos phase must say so, and the tier that runs it must not.
    ///
    /// This is the control for #2170: before it, a Quick run reported `bypassed=0` over a
    /// programme that never attempted a whole phase, and a reader could not tell that from a
    /// run that attempted everything.
    #[test]
    fn tiers_that_skip_chaos_declare_it() {
        for tier in [SuiteTier::Quick, SuiteTier::Nightly, SuiteTier::Stress] {
            assert_eq!(
                phases_not_attempted_for(&tier),
                vec!["chaos".to_string()],
                "{tier:?} does not run the chaos phase and must declare it"
            );
        }
        assert!(
            phases_not_attempted_for(&SuiteTier::Chaos).is_empty(),
            "the Chaos tier runs every phase, so it declares no omission"
        );
    }

    /// Pins the declaration to the gate that causes it.
    ///
    /// `run_suite` gates the chaos phase on `matches!(cfg.tier, SuiteTier::Chaos)`. If that gate
    /// moves, this reads the source and fails, rather than leaving the declaration describing a
    /// programme the code no longer runs.
    ///
    /// Only the code above `#[cfg(test)]` is searched. This test lives in the file it reads, so a
    /// whole-file search would be satisfied by this test's own literal — the assertion would hold
    /// even after the real gate moved, which is precisely the failure it exists to prevent.
    #[test]
    fn chaos_phase_gate_matches_the_declared_omission() {
        let source = include_str!("suite.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .expect("suite.rs keeps its test module")
            .0;
        assert!(
            production.contains("if matches!(cfg.tier, SuiteTier::Chaos) {"),
            "the chaos gate moved; phases_not_attempted_for must be updated with it"
        );
    }
}
