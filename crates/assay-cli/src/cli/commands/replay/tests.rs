use super::manifest::offline_dependency_message;
use super::provenance::annotate_run_json_provenance;
use super::run_args::replay_run_args;
use crate::exit_codes::ExitCodeVersion;
use assay_core::replay::{ReplayCoverage, ReplayManifest};
use std::path::PathBuf;

#[test]
fn offline_dependency_message_present_when_incomplete() {
    let mut manifest = ReplayManifest::minimal("2.15.0".to_string());
    manifest.replay_coverage = Some(ReplayCoverage {
        complete_tests: vec!["a".to_string()],
        incomplete_tests: vec!["b".to_string()],
        reason: Some(std::collections::BTreeMap::from([(
            "b".to_string(),
            "judge cache missing".to_string(),
        )])),
    });

    let msg = offline_dependency_message(&manifest).expect("message expected");
    assert!(msg.contains("incomplete"));
    assert!(msg.contains("b"));
}

#[test]
fn annotate_run_json_provenance_adds_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.json");
    std::fs::write(&path, r#"{"exit_code":0,"reason_code":""}"#).unwrap();

    annotate_run_json_provenance(&path, "sha256:abc", "offline", Some("123")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();

    assert_eq!(value["provenance"]["replay"], true);
    assert_eq!(value["provenance"]["bundle_digest"], "sha256:abc");
    assert_eq!(value["provenance"]["replay_mode"], "offline");
    assert_eq!(value["provenance"]["source_run_id"], "123");
}

#[test]
fn replay_run_args_overrides_and_inherits_defaults() {
    let args = replay_run_args(
        PathBuf::from("custom/eval.yaml"),
        Some(PathBuf::from("custom/trace.jsonl")),
        PathBuf::from("custom/eval.db"),
        true,
        ExitCodeVersion::V1,
    );

    assert_eq!(args.config, PathBuf::from("custom/eval.yaml"));
    assert_eq!(args.trace_file, Some(PathBuf::from("custom/trace.jsonl")));
    assert_eq!(args.db, PathBuf::from("custom/eval.db"));
    assert_eq!(args.quarantine_mode, "off");
    assert!(args.refresh_cache);
    assert!(args.no_cache);
    assert!(args.judge.no_judge);
    assert!(args.replay_strict);
    assert_eq!(args.exit_codes, ExitCodeVersion::V1);

    // Inherited from RunArgs defaults.
    assert_eq!(args.embedder, "none");
    assert_eq!(args.embedding_model, "text-embedding-3-small");
    assert!(!args.strict);
    assert!(!args.redact_prompts);
    assert!(!args.no_verify);
}
