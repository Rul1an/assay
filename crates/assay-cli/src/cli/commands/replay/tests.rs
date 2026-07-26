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

/// The public shape of a replay ingest refusal.
///
/// `E_REPLAY_LIMIT_EXCEEDED` exists so a resource refusal is distinguishable from a parse failure.
/// A bundle can be perfectly well-formed and merely larger than the configured budget, which is an
/// operator decision; reporting both as `E_CFG_PARSE` sent a reader off to fix the producer either
/// way. These tests pin what a consumer of `summary.json` actually sees, which is the contract —
/// the routing itself is pinned in `assay_core::replay::bundle::tests`.
mod replay_limit_summary_contract {
    use crate::cli::commands::run_output::summary_from_outcome;
    use crate::exit_codes::{ExitCodeVersion, ReasonCode, RunOutcome};
    use assay_common::limits::LimitKind;
    use assay_core::replay::bundle::ReplayIngestError;

    /// Reproduce exactly what `write_replay_failure` composes, minus the file IO. That function
    /// writes to fixed relative paths, so exercising it directly would mean moving the process
    /// CWD; the value it serializes is the part a consumer depends on.
    fn summary_for(refusal: &ReplayIngestError, digest: &str) -> serde_json::Value {
        let reason = ReasonCode::EReplayLimitExceeded;
        let message = format!("replay bundle refused by an ingest ceiling: {refusal}");
        let mut outcome = RunOutcome::from_reason(reason, Some(message), None);
        outcome.exit_code = reason.exit_code_for(ExitCodeVersion::default());

        let summary = summary_from_outcome(&outcome, true)
            .with_seeds(None, None)
            .with_replay_provenance(digest.to_string(), "offline", None);
        serde_json::to_value(&summary).expect("summary serializes")
    }

    fn every_variant() -> Vec<ReplayIngestError> {
        vec![
            ReplayIngestError::SourceCeiling {
                kind: LimitKind::SourceBytes,
                limit: 100 * 1024 * 1024,
            },
            ReplayIngestError::MemberCeiling {
                kind: LimitKind::MemberBytes,
                limit: 500 * 1024 * 1024,
            },
            ReplayIngestError::PathTooLong { limit: 256 },
            ReplayIngestError::TooManyEntries { limit: 100_000 },
            ReplayIngestError::ManifestTooDeep { limit: 64 },
        ]
    }

    /// The wire string, not the Rust identifier. Renaming the variant is free; renaming this
    /// breaks every consumer that greps a summary.
    #[test]
    fn the_reason_code_reaches_the_summary_verbatim() {
        for refusal in every_variant() {
            let v = summary_for(&refusal, "sha256:abc");
            assert_eq!(
                v["reason_code"], "E_REPLAY_LIMIT_EXCEEDED",
                "every ingest ceiling reports the same public code: {v}"
            );
        }
    }

    /// A resource refusal is classified at exit code 2 alongside the other operator-fixable
    /// conditions. It is not an infra failure and not a test failure.
    #[test]
    fn a_ceiling_refusal_is_a_config_class_exit() {
        let v = summary_for(&every_variant()[0], "sha256:abc");
        assert_eq!(v["exit_code"], 2, "{v}");
    }

    /// The refusal classifies the resource problem and adds nothing else. The reader never got far
    /// enough to form a judgement about the bundle's contents, so the summary must not imply one.
    #[test]
    fn the_refusal_carries_no_verdict() {
        let v = summary_for(&every_variant()[0], "sha256:abc");
        // Not "zero passed" — absent. A count of zero is itself a claim about a run that never
        // started, and a consumer aggregating summaries would fold it in as a real result.
        for claim in ["passed", "failed", "total", "verdict", "score", "compliant"] {
            assert!(
                v.get(claim).is_none(),
                "a ceiling refusal must not assert `{claim}`; no test ran: {v}"
            );
        }
    }

    /// Once the source is snapshotted we hold the digest of the exact bytes that failed. Losing it
    /// to `sha256:unknown` was the defect the typed refusal path fixed, and the digest is what
    /// makes the refusal reproducible by someone else.
    #[test]
    fn the_digest_of_the_failing_bytes_survives() {
        let v = summary_for(&every_variant()[0], "sha256:deadbeef");
        let found = v.to_string().contains("sha256:deadbeef");
        assert!(found, "the source digest must reach the summary: {v}");
        assert!(
            !v.to_string().contains("sha256:unknown"),
            "the digest must not degrade to unknown when we hold it: {v}"
        );
    }

    /// Value-free: the operator-facing message names the configured ceiling and the dimension, and
    /// carries nothing the archive chose. A member name or a recorded length echoed back here is
    /// attacker-controlled text in an operator's terminal and in every log that ingests it.
    #[test]
    fn the_message_reports_the_ceiling_and_nothing_the_archive_chose() {
        for refusal in every_variant() {
            let rendered = refusal.to_string();
            let v = summary_for(&refusal, "sha256:abc");
            let message = v["message"].as_str().unwrap_or_default();

            assert!(
                message.contains(&rendered),
                "the typed refusal must be what is rendered: {message}"
            );
            // Each variant's Display is built only from its own fields, and those fields are the
            // configured limit plus a `LimitKind` tag. Nothing is threaded through from the
            // archive, so there is no path for archive bytes to reach this string.
            for archive_controlled in ["../", "\u{1b}", "payload", ".tar", "manifest.json"] {
                assert!(
                    !message.contains(archive_controlled),
                    "archive-controlled text `{archive_controlled}` reached the message: {message}"
                );
            }
        }
    }

    /// The remedy names the operator's lever. A ceiling refusal is the one failure class where
    /// "raise the budget" is a legitimate answer, and the next step has to say so rather than
    /// sending the reader to the producer.
    #[test]
    fn the_next_step_points_at_the_budget() {
        let v = summary_for(&every_variant()[0], "sha256:abc");
        let next = v["next_step"].as_str().unwrap_or_default();
        assert!(
            next.contains("ceiling") || next.contains("smaller bundle"),
            "next step must name the budget lever: {next}"
        );
    }
}
