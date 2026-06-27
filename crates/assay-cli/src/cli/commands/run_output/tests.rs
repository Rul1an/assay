use super::{
    decide_run_outcome, has_judge_verdict_abstain, reason_code_from_error_message,
    reason_code_from_run_error, write_extended_run_json, write_run_json_minimal,
};
use crate::exit_codes::{ExitCodeVersion, ReasonCode, RunOutcome, EXIT_INFRA_ERROR};
use assay_core::errors::RunError;
use assay_core::model::{TestResultRow, TestStatus};
use assay_core::report::RunArtifacts;

#[test]
fn on_disk_run_json_is_render_safe() {
    let secret = format!("ghp_{}", "A".repeat(36));
    let artifacts = RunArtifacts {
        run_id: 7,
        suite: "owned-suite".into(),
        results: vec![TestResultRow {
            test_id: "t_owned".into(),
            status: TestStatus::Fail,
            score: Some(0.0),
            cached: false,
            message: format!("failed: leaked {secret}"),
            details: serde_json::json!({
                "prompt": format!("ask {secret} alice@example.com"),
                "metrics": { "must_contain": { "details": { "message": format!("missing {secret}") } } },
            }),
            duration_ms: Some(3),
            fingerprint: Some("fp_owned".into()),
            skip_reason: None,
            attempts: None,
            error_policy_applied: None,
        }],
        order_seed: None,
        runner_clone_ms: None,
    };
    let outcome = RunOutcome::from_reason(ReasonCode::ETestFailed, None, None);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.json");
    write_extended_run_json(&artifacts, &outcome, &path, None).unwrap();
    let out = std::fs::read_to_string(&path).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(!out.contains(&secret), "on-disk run.json leaked secret");
    assert!(!out.contains("ghp_"), "raw token prefix leaked to run.json");
    assert!(
        !out.contains("alice@example.com"),
        "on-disk run.json leaked pii"
    );
    assert!(
        out.contains("<redacted:"),
        "on-disk run.json fired no redaction"
    );
    assert_eq!(parsed["suite"], "owned-suite");
    assert_eq!(parsed["results"][0]["test_id"], "t_owned");
    assert_eq!(parsed["results"][0]["fingerprint"], "fp_owned");
    assert_eq!(parsed["reason_code"], "E_TEST_FAILED");
}

#[test]
fn minimal_run_json_resolution_message_is_render_safe() {
    let secret = format!("ghp_{}", "Z".repeat(36));
    let outcome = RunOutcome::from_reason(
        ReasonCode::ETraceNotFound,
        Some(format!("trace not found: {secret}")),
        None,
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.json");
    write_run_json_minimal(&outcome, &path).unwrap();
    let out = std::fs::read_to_string(&path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(
        !out.contains(&secret),
        "minimal run.json leaked secret in resolution.message"
    );
    assert!(!out.contains("ghp_"), "raw token prefix leaked to run.json");
    assert!(
        out.contains("<redacted:"),
        "minimal run.json fired no redaction"
    );
    assert_eq!(parsed["reason_code"], "E_TRACE_NOT_FOUND");
}

#[test]
fn test_infra_beats_abstain_precedence() {
    let results = vec![
        TestResultRow {
            test_id: "infra".into(),
            status: TestStatus::Error,
            score: None,
            cached: false,
            message: "Request timeout".into(),
            details: serde_json::json!({}),
            duration_ms: None,
            fingerprint: None,
            skip_reason: None,
            attempts: None,
            error_policy_applied: None,
        },
        TestResultRow {
            test_id: "abstain".into(),
            status: TestStatus::Pass,
            score: Some(0.5),
            cached: false,
            message: String::new(),
            details: serde_json::json!({
                "metrics": {
                    "faithfulness": {
                        "details": { "verdict": "Abstain", "score": 0.5 }
                    }
                }
            }),
            duration_ms: None,
            fingerprint: None,
            skip_reason: None,
            attempts: None,
            error_policy_applied: None,
        },
    ];
    let outcome = decide_run_outcome(&results, false, ExitCodeVersion::V2);
    assert_eq!(
        outcome.exit_code, EXIT_INFRA_ERROR,
        "infra must beat abstain: expected exit 3"
    );
    assert!(
        outcome.reason_code == ReasonCode::ETimeout.as_str()
            || outcome.reason_code == ReasonCode::EJudgeUnavailable.as_str(),
        "reason should be infra (E_TIMEOUT or E_JUDGE_UNAVAILABLE), got {}",
        outcome.reason_code
    );
}

#[test]
fn test_has_judge_verdict_abstain_detects_abstain() {
    let details = serde_json::json!({
        "metrics": {
            "faithfulness": {
                "score": 0.5,
                "passed": false,
                "unstable": true,
                "details": { "verdict": "Abstain", "score": 0.5 }
            }
        }
    });
    assert!(has_judge_verdict_abstain(&details));
}

#[test]
fn test_has_judge_verdict_abstain_ignores_pass() {
    let details = serde_json::json!({
        "metrics": {
            "faithfulness": {
                "score": 1.0,
                "passed": true,
                "unstable": false,
                "details": { "verdict": "Pass", "score": 1.0 }
            }
        }
    });
    assert!(!has_judge_verdict_abstain(&details));
}

#[test]
fn test_has_judge_verdict_abstain_no_metrics() {
    let details = serde_json::json!({});
    assert!(!has_judge_verdict_abstain(&details));
}

#[test]
fn test_reason_code_from_error_message_maps_config_family() {
    assert_eq!(
        reason_code_from_error_message("trace not found: traces/missing.jsonl"),
        Some(ReasonCode::ETraceNotFound)
    );
    assert_eq!(
        reason_code_from_error_message("Config file not found: eval.yaml"),
        Some(ReasonCode::EMissingConfig)
    );
    assert_eq!(
        reason_code_from_error_message("config error: unknown field `foo`"),
        Some(ReasonCode::ECfgParse)
    );
}

#[test]
fn test_reason_code_from_error_message_maps_infra_family() {
    assert_eq!(
        reason_code_from_error_message("provider returned 429 rate limit"),
        Some(ReasonCode::ERateLimit)
    );
    assert_eq!(
        reason_code_from_error_message("request timeout while calling judge"),
        Some(ReasonCode::ETimeout)
    );
    assert_eq!(
        reason_code_from_error_message("provider error: 503"),
        Some(ReasonCode::EProvider5xx)
    );
    assert_eq!(
        reason_code_from_error_message("network connection reset by peer"),
        Some(ReasonCode::ENetworkError)
    );
}

#[test]
fn test_reason_code_from_run_error_uses_typed_kind() {
    let typed = RunError::missing_config("eval.yaml", "missing");
    assert_eq!(
        reason_code_from_run_error(&typed),
        Some(ReasonCode::EMissingConfig)
    );
    assert!(!typed.legacy_classified);
}

#[test]
fn test_decide_outcome_uses_typed_details_before_legacy_message_fallback() {
    let row = TestResultRow {
        test_id: "typed".into(),
        status: TestStatus::Error,
        score: None,
        cached: false,
        message: "untyped error text".into(),
        details: serde_json::json!({
            "run_error_kind": "invalid_args"
        }),
        duration_ms: None,
        fingerprint: None,
        skip_reason: None,
        attempts: None,
        error_policy_applied: None,
    };

    let outcome = decide_run_outcome(&[row], true, ExitCodeVersion::V2);
    assert_eq!(outcome.reason_code, ReasonCode::EInvalidArgs.as_str());
    assert_eq!(
        outcome.exit_code,
        ReasonCode::EInvalidArgs.exit_code_for(ExitCodeVersion::V2)
    );
}
