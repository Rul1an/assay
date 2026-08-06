#![cfg(test)]

//! No tests moved in Step2 Commit A.
//! Step1 monitor contract tests remain in:
//! `crates/assay-cli/src/cli/commands/monitor.rs`.
//! See move-map/checklist artifacts in Commit C.

#[test]
fn enforcement_failure_exit_distinguishes_artifact_write_failure() {
    assert_eq!(
        super::enforcement_failure_exit(true, crate::exit_codes::EXIT_WOULD_BLOCK),
        crate::exit_codes::EXIT_WOULD_BLOCK
    );
    assert_eq!(
        super::enforcement_failure_exit(false, crate::exit_codes::EXIT_WOULD_BLOCK),
        crate::exit_codes::EXIT_INFRA_ERROR
    );
}

#[test]
fn enforcement_failure_exit_preserves_runtime_failure_and_prioritizes_carrier_failure() {
    assert_eq!(super::enforcement_failure_exit(true, 40), 40);
    assert_eq!(
        super::enforcement_failure_exit(false, 40),
        crate::exit_codes::EXIT_INFRA_ERROR
    );
    assert_eq!(
        super::enforcement_failure_exit(true, crate::exit_codes::EXIT_WOULD_BLOCK),
        crate::exit_codes::EXIT_WOULD_BLOCK
    );
}

#[test]
fn startup_failure_health_distinguishes_requested_network_enforcement() {
    use super::enforcement_health::NetworkEnforcement;

    assert_eq!(
        super::startup_failure_health(false).network_enforcement,
        NetworkEnforcement::Absent
    );
    assert_eq!(
        super::startup_failure_health(true).network_enforcement,
        NetworkEnforcement::Failed
    );
}

#[test]
fn tier1_enforcement_detection_includes_file_only_policies() {
    let mut compiled = assay_policy::tiers::CompiledPolicy {
        tier1: assay_policy::tiers::Tier1Rules::default(),
        tier2: assay_policy::tiers::Tier2Rules::default(),
        stats: assay_policy::tiers::CompilationStats::default(),
    };
    assert!(!super::tier1_enforcement_requested(&compiled));

    compiled
        .tier1
        .file_deny_prefix
        .push(assay_policy::tiers::PathRule {
            rule_id: 1,
            path: "/sensitive".to_string(),
            hash: 0,
        });
    assert!(super::tier1_enforcement_requested(&compiled));
}

fn monitor_args_with_health(
    path: std::path::PathBuf,
) -> crate::cli::commands::monitor::MonitorArgs {
    crate::cli::commands::monitor::MonitorArgs {
        pid: Vec::new(),
        ebpf: None,
        print: false,
        quiet: true,
        duration: None,
        policy: None,
        monitor_all: false,
        enforcement_health: Some(path),
        observed_peers: None,
        observation_health: None,
    }
}

fn file_only_tier1_policy() -> assay_policy::tiers::CompiledPolicy {
    let mut compiled = assay_policy::tiers::CompiledPolicy {
        tier1: assay_policy::tiers::Tier1Rules::default(),
        tier2: assay_policy::tiers::Tier2Rules::default(),
        stats: assay_policy::tiers::CompilationStats::default(),
    };
    compiled
        .tier1
        .file_deny_prefix
        .push(assay_policy::tiers::PathRule {
            rule_id: 1,
            path: "/sensitive".to_string(),
            hash: 0,
        });
    compiled
}

#[test]
fn file_only_tier1_install_failure_refuses_and_retains_absent_network_health() {
    let output_dir = tempfile::TempDir::new().expect("temp output dir");
    let health_path = output_dir.path().join("enforcement-health.json");
    let args = monitor_args_with_health(health_path.clone());

    assert_eq!(
        super::tier1_install_failure_exit(
            &args,
            &file_only_tier1_policy(),
            crate::exit_codes::EXIT_WOULD_BLOCK,
        ),
        Some(crate::exit_codes::EXIT_WOULD_BLOCK)
    );

    let health: serde_json::Value = serde_json::from_slice(
        &std::fs::read(health_path).expect("read retained enforcement health"),
    )
    .expect("parse retained enforcement health");
    assert_eq!(health["network_enforcement"], "absent");
}

#[test]
fn file_only_tier1_install_failure_prioritizes_carrier_write_failure() {
    let unwritable_target = tempfile::TempDir::new().expect("directory is not a file");
    let args = monitor_args_with_health(unwritable_target.path().to_path_buf());

    assert_eq!(
        super::tier1_install_failure_exit(
            &args,
            &file_only_tier1_policy(),
            crate::exit_codes::EXIT_WOULD_BLOCK,
        ),
        Some(crate::exit_codes::EXIT_INFRA_ERROR)
    );
}
