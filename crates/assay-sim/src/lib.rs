pub mod attacks;
pub mod corpus;
pub mod differential;
pub mod mutators;
pub mod report;
pub mod subprocess;
pub mod suite;

pub use report::{AttackResult, AttackStatus, SimReport};
pub use suite::{run_suite, tier_default_limits, SuiteConfig, SuiteTier, TimeBudget};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_quick_suite() {
        let cfg = SuiteConfig {
            tier: SuiteTier::Quick,
            target_bundle: PathBuf::from("placeholder"),
            seed: 42,
            verify_limits: None,
            time_budget_secs: 60,
        };

        let report = run_suite(cfg).expect("Suite failed to run");

        // Print full report on failure for debugging
        if report.summary.bypassed > 0 {
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }

        // Invariant assertions (stable across attack additions):
        // - No attack may bypass verification (security contract)
        assert_eq!(
            report.summary.bypassed, 0,
            "SECURITY: {} attacks bypassed verification",
            report.summary.bypassed
        );
        // - At least 1 attack must be blocked (sanity: attacks actually ran)
        assert!(
            report.summary.blocked >= 1,
            "SANITY: no attacks were blocked — suite may not have run"
        );
        // - At least 1 check must pass, or differential ran (sanity: differential tests ran; allow flaky fail on CI)
        let differential_ran = report
            .results
            .iter()
            .any(|r| r.name == "differential.invariants");
        assert!(
            report.summary.passed >= 1 || differential_ran,
            "SANITY: no checks passed and differential did not run — suite may not have run"
        );
        // - Every result must have a valid status classification:
        //   Blocked/Passed are normal outcomes.
        //   Error is acceptable for chaos IO faults (WouldBlock, persistent EINTR)
        //   but NOT for integrity/differential tests.
        for r in &report.results {
            let is_chaos_io = r.name.starts_with("chaos.io_fault.");
            match r.status {
                AttackStatus::Blocked | AttackStatus::Passed => {} // always ok
                AttackStatus::Error if is_chaos_io => {}           // infra IO, acceptable
                _ => panic!(
                    "Unexpected status {:?} for '{}': {:?}",
                    r.status, r.name, r.message
                ),
            }
        }
    }
}
