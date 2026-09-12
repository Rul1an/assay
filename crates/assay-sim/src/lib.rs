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

    /// The refusal each Quick-tier name is supposed to reach. A green `bypassed=0` is not
    /// enough: an earlier check can refuse the fixture and the named rule can then regress
    /// unnoticed. `integrity.bitflip` at seed 42 lands wherever the flips land, so only its
    /// class is pinned; `integrity.bitflip_crc` pins the trailer drain. CRLF is tolerated by
    /// the verifier, so that name is an invariant rather than an attack.
    const QUICK_REFUSALS: &[QuickRefusal] = &[
        QuickRefusal::class("integrity.bitflip", "Integrity"),
        QuickRefusal::code_and_message(
            "integrity.bitflip_crc",
            "Integrity",
            "IntegrityGzip",
            "Gzip trailer",
        ),
        QuickRefusal::code("integrity.truncate", "Integrity", "IntegrityIo"),
        QuickRefusal::code(
            "integrity.inject_file",
            "Contract",
            "ContractUnexpectedFile",
        ),
        QuickRefusal::code("security.zip_bomb", "Limits", "LimitDecodeBytes"),
        QuickRefusal::code(
            "integrity.tar_duplicate",
            "Contract",
            "ContractDuplicateFile",
        ),
        QuickRefusal::code_and_message(
            "integrity.ndjson_bom",
            "Contract",
            "ContractInvalidJson",
            "BOM not allowed",
        ),
        QuickRefusal::passed("integrity.ndjson_crlf"),
        QuickRefusal::code("integrity.limit_bundle_bytes", "Limits", "LimitBundleBytes"),
        QuickRefusal::passed("differential.invariants"),
    ];

    struct QuickRefusal {
        name: &'static str,
        status: AttackStatus,
        class: Option<&'static str>,
        code: Option<&'static str>,
        message_contains: Option<&'static str>,
    }

    impl QuickRefusal {
        const fn class(name: &'static str, class: &'static str) -> Self {
            Self {
                name,
                status: AttackStatus::Blocked,
                class: Some(class),
                code: None,
                message_contains: None,
            }
        }

        const fn code(name: &'static str, class: &'static str, code: &'static str) -> Self {
            Self {
                name,
                status: AttackStatus::Blocked,
                class: Some(class),
                code: Some(code),
                message_contains: None,
            }
        }

        const fn code_and_message(
            name: &'static str,
            class: &'static str,
            code: &'static str,
            message_contains: &'static str,
        ) -> Self {
            Self {
                name,
                status: AttackStatus::Blocked,
                class: Some(class),
                code: Some(code),
                message_contains: Some(message_contains),
            }
        }

        const fn passed(name: &'static str) -> Self {
            Self {
                name,
                status: AttackStatus::Passed,
                class: None,
                code: None,
                message_contains: None,
            }
        }
    }

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

        assert_named_quick_refusals(&report);
    }

    fn assert_named_quick_refusals(report: &SimReport) {
        for expected in QUICK_REFUSALS {
            let r = report
                .results
                .iter()
                .find(|r| r.name == expected.name)
                .unwrap_or_else(|| panic!("Quick suite is missing '{}'", expected.name));
            assert_eq!(
                r.status, expected.status,
                "{}: status {:?} (message {:?})",
                expected.name, r.status, r.message
            );
            if let Some(class) = expected.class {
                assert_eq!(
                    r.error_class.as_deref(),
                    Some(class),
                    "{}: refusing class",
                    expected.name
                );
            }
            if let Some(code) = expected.code {
                assert_eq!(
                    r.error_code.as_deref(),
                    Some(code),
                    "{}: refusing code (message {:?})",
                    expected.name,
                    r.message
                );
            }
            if let Some(part) = expected.message_contains {
                let message = r.message.as_deref().unwrap_or("");
                assert!(
                    message.contains(part),
                    "{}: message {message:?} should name {part:?}",
                    expected.name
                );
            }
        }

        for r in &report.results {
            assert!(
                QUICK_REFUSALS
                    .iter()
                    .any(|expected| expected.name == r.name),
                "Quick suite result '{}' has no pinned refusal",
                r.name
            );
        }
    }
}
