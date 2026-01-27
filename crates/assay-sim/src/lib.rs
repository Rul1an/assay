pub mod attacks;
pub mod corpus;
pub mod differential;
pub mod mutators;
pub mod report;
pub mod suite;

pub use report::{AttackResult, AttackStatus, SimReport};
pub use suite::{run_suite, SuiteConfig, SuiteTier};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_quick_suite() {
        let cfg = SuiteConfig {
            tier: SuiteTier::Quick,
            target_bundle: PathBuf::from("placeholder"), // dynamic generation in suite for now
            seed: 42,
            verify_limits: None,
        };

        let report = run_suite(cfg).expect("Suite failed to run");

        // Assert Summary
        println!("Report Summary: {:?}", report.summary);

        // We expect:
        // 1. attacks checks (BitFlip, Truncate, Inject, ZipBomb, TarDuplicate, Bom, Crlf, BundleSize) -> Blocked (count=8)
        // 2. differential -> Passed (count=1)
        // Total = 9, Blocked=8, Passed=1, Bypassed=0
        if report.summary.blocked != 8 || report.summary.passed != 1 {
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        assert_eq!(report.summary.blocked, 8, "Expected 8 blocked attacks");
        assert_eq!(report.summary.passed, 1, "Expected 1 passed check");
        assert_eq!(report.summary.bypassed, 0, "Expected 0 bypassed attacks");
    }
}
