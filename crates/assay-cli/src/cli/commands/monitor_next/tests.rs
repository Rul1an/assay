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

#[cfg(feature = "runner")]
#[test]
fn one_monitor_invocation_shares_one_id_across_observation_artifacts() {
    let output_dir = tempfile::TempDir::new().expect("temporary artifact directory");
    let peers_path = output_dir.path().join("observed-peers.json");
    let health_path = output_dir.path().join("observation-health.json");
    let run_id = super::observation_health::new_run_id();
    let artifacts = super::build_observation_artifacts(
        &run_id,
        &assay_monitor::probes::ProbeAttachment::default(),
        0,
        false,
        vec!["127.0.0.1:443".to_string()],
    );
    artifacts
        .observed_peers
        .write_to(&peers_path)
        .expect("write observed peers");
    assert!(super::observation_health::write_to(
        &artifacts.observation_health,
        &health_path
    ));

    let peers: super::observed_peers::ObservedPeers =
        serde_json::from_slice(&std::fs::read(peers_path).expect("read observed peers"))
            .expect("parse observed peers");
    let health: assay_runner_schema::ObservationHealth =
        serde_json::from_slice(&std::fs::read(health_path).expect("read observation health"))
            .expect("parse observation health");

    assert_eq!(
        peers.run_id, health.run_id,
        "crossed artifacts must be rejected rather than generated by one invocation"
    );
}

#[cfg(feature = "runner")]
#[test]
fn preparing_observation_outputs_removes_both_stale_counterparts() {
    let output_dir = tempfile::TempDir::new().expect("temporary artifact directory");
    let peers_path = output_dir.path().join("observed-peers.json");
    let health_path = output_dir.path().join("observation-health.json");
    std::fs::write(&peers_path, "old peers").expect("write stale peers");
    std::fs::write(&health_path, "old health").expect("write stale health");

    super::prepare_observation_artifact_targets(Some(&peers_path), Some(&health_path))
        .expect("prepare artifact targets");

    assert!(
        !peers_path.exists(),
        "stale peers must not survive a new run"
    );
    assert!(
        !health_path.exists(),
        "stale health must not pair with a partially written new run"
    );
}

#[cfg(feature = "runner")]
#[test]
fn observation_outputs_refuse_the_same_target_path() {
    let output_dir = tempfile::TempDir::new().expect("temporary artifact directory");
    let path = output_dir.path().join("observation.json");

    let err = super::prepare_observation_artifact_targets(Some(&path), Some(&path))
        .expect_err("two artifact schemas cannot safely share one file");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(err.to_string().contains("different output paths"));
}

#[cfg(feature = "runner")]
#[test]
fn observation_outputs_refuse_aliases_to_the_same_absent_target() {
    let output_dir = tempfile::TempDir::new().expect("temporary artifact directory");
    let detour = output_dir.path().join("detour");
    std::fs::create_dir(&detour).expect("create path detour");
    let direct = output_dir.path().join("observation.json");
    let aliased = detour.join("..").join("observation.json");
    assert!(!direct.exists());
    assert!(!aliased.exists());

    let err = super::prepare_observation_artifact_targets(Some(&direct), Some(&aliased))
        .expect_err("lexically different paths must not overwrite one artifact target");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(err.to_string().contains("different output paths"));
}
