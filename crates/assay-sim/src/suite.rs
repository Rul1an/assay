use crate::attacks;
use crate::differential;
use crate::report::SimReport;
use anyhow::Result;
use assay_evidence::VerifyLimits;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum SuiteTier {
    Quick,
    Nightly,
    Stress,
}

#[derive(Debug, Clone)]
pub struct SuiteConfig {
    pub tier: SuiteTier,
    pub target_bundle: PathBuf, // Placeholder for future file-based targets
    pub seed: u64,
    pub verify_limits: Option<VerifyLimits>,
}

pub fn run_suite(cfg: SuiteConfig) -> Result<SimReport> {
    let mut report = SimReport::new(&format!("{:?}", cfg.tier), cfg.seed);

    // 1. Integrity Attacks
    attacks::integrity::check_integrity_attacks(&mut report)?;

    // 2. Differential Testing
    let iterations = match cfg.tier {
        SuiteTier::Quick => 5,
        SuiteTier::Nightly => 100,
        SuiteTier::Stress => 1000,
    };

    let start = std::time::Instant::now();
    let res = differential::check_invariants(iterations, Some(cfg.seed));
    let duration = start.elapsed().as_millis() as u64;

    report.add_check("differential.invariants", res, duration);

    Ok(report)
}
