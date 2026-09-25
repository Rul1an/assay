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

/// A passing row whose named metric evaluated nothing, shaped as `single.rs` writes it.
fn not_exercised_row(test_id: &str, metric: &str, reason: &str) -> TestResultRow {
    TestResultRow {
        test_id: test_id.into(),
        status: TestStatus::Pass,
        score: None,
        cached: false,
        message: "ok".into(),
        details: serde_json::json!({
            "metrics": {
                metric: {
                    "score": 1.0,
                    "passed": true,
                    "unstable": false,
                    "exercised": "not_exercised",
                    "details": { "reason": reason }
                }
            }
        }),
        duration_ms: Some(1),
        fingerprint: None,
        skip_reason: None,
        attempts: None,
        error_policy_applied: None,
    }
}

/// The success path is the one that matters, and it is the one an early `return` would have missed.
///
/// `decide_run_outcome` has six early returns; a not-exercised metric appears on a *green* run by
/// definition, so the warning must survive the path that returns first and reports nothing else.
#[test]
fn a_not_exercised_metric_warns_on_a_passing_run_without_changing_the_exit_code() {
    let rows = vec![not_exercised_row(
        "t1",
        "sequence_valid",
        "no tool calls in the trace",
    )];
    let outcome = decide_run_outcome(&rows, false, ExitCodeVersion::default());

    assert_eq!(outcome.exit_code, 0, "a coverage observation is not a gate");
    assert_eq!(outcome.reason_code, ReasonCode::Success.as_str());
    assert!(
        outcome
            .warnings
            .iter()
            .any(|w| w.contains("W_METRIC_NOT_EXERCISED") && w.contains("sequence_valid")),
        "warnings: {:?}",
        outcome.warnings
    );
}

/// And on a failing run too — the early return at priority 1/2 must not swallow it.
#[test]
fn a_failing_run_still_reports_what_was_never_exercised() {
    let mut fail = not_exercised_row("t2", "tool_output_valid", "no output schemas configured");
    fail.status = TestStatus::Fail;
    fail.message = "failed: must_contain".into();
    let rows = vec![
        not_exercised_row("t1", "tool_output_valid", "no output schemas configured"),
        fail,
    ];
    let outcome = decide_run_outcome(&rows, false, ExitCodeVersion::default());

    assert_ne!(outcome.exit_code, 0, "the failure still decides the exit");
    assert!(
        outcome.warnings.iter().any(|w| w.contains("2 test(s)")),
        "the failing run lost its coverage warning: {:?}",
        outcome.warnings
    );
}

/// `--strict` promotes Warn/Flaky/Unstable *statuses* to a violation. A not-exercised metric is not
/// a status, so strict mode must not turn a coverage observation into a failing build — that is
/// precisely the over-eager detection that earns a suppression.
#[test]
fn strict_mode_does_not_promote_a_coverage_observation_to_a_violation() {
    let rows = vec![not_exercised_row("t1", "seq", "no tool calls")];
    let outcome = decide_run_outcome(&rows, true, ExitCodeVersion::default());
    assert_eq!(outcome.exit_code, 0, "strict mode failed a green suite");
    assert!(!outcome.warnings.is_empty());
}

/// The warning reaches the artifact, not just the struct.
#[test]
fn the_warning_lands_in_run_json() {
    let artifacts = RunArtifacts {
        run_id: 1,
        suite: "s".into(),
        results: vec![not_exercised_row("t1", "seq", "no tool calls in the trace")],
        order_seed: None,
        runner_clone_ms: None,
    };
    let outcome = decide_run_outcome(&artifacts.results, false, ExitCodeVersion::default());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.json");
    write_extended_run_json(&artifacts, &outcome, &path, None).unwrap();

    let parsed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let warnings = parsed["warnings"].as_array().expect("warnings array");
    assert!(
        warnings.iter().any(|w| w
            .as_str()
            .is_some_and(|s| s.contains("W_METRIC_NOT_EXERCISED"))),
        "run.json: {parsed}"
    );
}

/// A run with nothing to say adds no `warnings` key at all.
#[test]
fn a_fully_exercised_run_adds_no_warnings() {
    let mut row = not_exercised_row("t1", "semantic", "n/a");
    row.details["metrics"]["semantic"]["exercised"] = serde_json::json!("exercised");
    let outcome = decide_run_outcome(&[row], false, ExitCodeVersion::default());
    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
}

/// A row whose assertions never evaluated, shaped as `runner_next/assertions`
/// writes it (#3117, 6.7.0).
fn not_evaluated_row(test_id: &str, kind: &str, remedy: &str) -> TestResultRow {
    TestResultRow {
        test_id: test_id.into(),
        status: TestStatus::Error,
        score: None,
        cached: false,
        message: format!("assertions error: E_TRACE_EPISODE_...: no episode for {test_id}"),
        details: serde_json::json!({
            "assertions": { "error": format!("E_TRACE_EPISODE_...: no episode for {test_id}") },
            "assertions_not_evaluated": {
                "evaluated": false,
                "kind": kind,
                "remedy": remedy,
            },
        }),
        duration_ms: None,
        fingerprint: None,
        skip_reason: None,
        attempts: None,
        error_policy_applied: None,
    }
}

fn infra_error_row(test_id: &str) -> TestResultRow {
    TestResultRow {
        test_id: test_id.into(),
        status: TestStatus::Error,
        score: None,
        cached: false,
        message: "provider returned 429 rate limit".into(),
        details: serde_json::json!({
            "run_error_kind": "provider_rate_limit"
        }),
        duration_ms: None,
        fingerprint: None,
        skip_reason: None,
        attempts: None,
        error_policy_applied: None,
    }
}

fn evaluated_fail_row(test_id: &str) -> TestResultRow {
    TestResultRow {
        test_id: test_id.into(),
        status: TestStatus::Fail,
        score: None,
        cached: false,
        message: "assertions failed (1)".into(),
        details: serde_json::json!({
            "assertions": [{ "message": "must not call delete_repository" }]
        }),
        duration_ms: None,
        fingerprint: None,
        skip_reason: None,
        attempts: None,
        error_policy_applied: None,
    }
}

/// B1: a missing episode exits 1 with its registered code on both profiles —
/// and never as `E_JUDGE_UNAVAILABLE`, which is what Priority 2 would report
/// for an `Error` row without the not-evaluated exclusion.
#[test]
fn missing_episode_reports_its_code_at_exit_1_on_both_profiles() {
    use assay_core::report::exercised::EPISODE_MISSING;
    let rows = || {
        vec![not_evaluated_row(
            "t_missing",
            EPISODE_MISSING,
            "the episode's meta.test_id must match the suite test id",
        )]
    };
    for version in [ExitCodeVersion::V1, ExitCodeVersion::V2] {
        let outcome = decide_run_outcome(&rows(), false, version);
        assert_eq!(
            outcome.reason_code,
            ReasonCode::ETraceEpisodeMissing.as_str(),
            "version {version:?}"
        );
        assert_eq!(outcome.exit_code, 1, "version {version:?}");
        assert!(
            outcome
                .next_step
                .as_deref()
                .is_some_and(|s| s.contains("meta.test_id")),
            "next_step is the row remedy: {:?}",
            outcome.next_step
        );
    }
}

/// B1: an ambiguous episode has its own code and the same exit class.
#[test]
fn ambiguous_episode_reports_its_code_at_exit_1() {
    use assay_core::report::exercised::EPISODE_AMBIGUOUS;
    let rows = vec![not_evaluated_row(
        "t_ambiguous",
        EPISODE_AMBIGUOUS,
        "keep a single stored episode whose meta.test_id is the suite test id",
    )];
    let outcome = decide_run_outcome(&rows, false, ExitCodeVersion::V2);
    assert_eq!(
        outcome.reason_code,
        ReasonCode::ETraceEpisodeAmbiguous.as_str()
    );
    assert_eq!(outcome.exit_code, 1);
}

/// B1: an evaluated failure is reported before a not-evaluated row.
#[test]
fn evaluated_failure_beats_not_evaluated_row() {
    use assay_core::report::exercised::EPISODE_MISSING;
    let rows = vec![
        evaluated_fail_row("t_fail"),
        not_evaluated_row(
            "t_missing",
            EPISODE_MISSING,
            "the episode's meta.test_id must match the suite test id",
        ),
    ];
    let outcome = decide_run_outcome(&rows, false, ExitCodeVersion::V2);
    assert_eq!(outcome.reason_code, ReasonCode::ETestFailed.as_str());
    assert_eq!(outcome.exit_code, 1);
}

/// Missing outranks ambiguous regardless of result order, with counts.
#[test]
fn missing_outranks_ambiguous_with_counts() {
    use assay_core::report::exercised::{EPISODE_AMBIGUOUS, EPISODE_MISSING};
    let rows = vec![
        not_evaluated_row(
            "t_ambiguous",
            EPISODE_AMBIGUOUS,
            "keep a single stored episode whose meta.test_id is the suite test id",
        ),
        not_evaluated_row(
            "t_missing",
            EPISODE_MISSING,
            "the episode's meta.test_id must match the suite test id",
        ),
    ];
    let outcome = decide_run_outcome(&rows, false, ExitCodeVersion::V2);
    assert_eq!(
        outcome.reason_code,
        ReasonCode::ETraceEpisodeMissing.as_str()
    );
    assert_eq!(
        outcome.message.as_deref(),
        Some("assertions not evaluated for 2 test(s): 1 missing, 1 ambiguous")
    );
}

/// B1: a genuine infra `Error` still precedes the not-evaluated step, so a
/// missing episode beside a rate limit reports the rate limit at exit 3.
#[test]
fn infra_error_beats_not_evaluated_row() {
    use assay_core::report::exercised::EPISODE_MISSING;
    let rows = vec![
        not_evaluated_row(
            "t_missing",
            EPISODE_MISSING,
            "the episode's meta.test_id must match the suite test id",
        ),
        infra_error_row("t_limited"),
    ];
    let outcome = decide_run_outcome(&rows, false, ExitCodeVersion::V2);
    assert_eq!(outcome.reason_code, ReasonCode::ERateLimit.as_str());
    assert_eq!(outcome.exit_code, 3);
}

/// The typed companion wins over a misleading message: the legacy classifier
/// is never consulted for not-evaluated rows.
#[test]
fn not_evaluated_companion_beats_legacy_message_classification() {
    use assay_core::report::exercised::EPISODE_MISSING;
    let mut row = not_evaluated_row(
        "t_missing",
        EPISODE_MISSING,
        "the episode's meta.test_id must match the suite test id",
    );
    row.message = "judge unavailable: connection reset".into();
    let outcome = decide_run_outcome(&[row], false, ExitCodeVersion::V2);
    assert_eq!(
        outcome.reason_code,
        ReasonCode::ETraceEpisodeMissing.as_str()
    );
    assert_eq!(outcome.exit_code, 1);
}
