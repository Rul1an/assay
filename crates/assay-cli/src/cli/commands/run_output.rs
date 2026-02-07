use crate::exit_codes::{ReasonCode, RunOutcome};
use std::path::{Path, PathBuf};

pub(crate) fn reason_code_from_error_message(message: &str) -> Option<ReasonCode> {
    use assay_core::errors::{RunError, RunErrorKind};

    let classified = RunError::classify_message(message.to_string());
    match classified.kind {
        RunErrorKind::TraceNotFound => Some(ReasonCode::ETraceNotFound),
        RunErrorKind::MissingConfig => Some(ReasonCode::EMissingConfig),
        RunErrorKind::ConfigParse => Some(ReasonCode::ECfgParse),
        RunErrorKind::InvalidArgs => Some(ReasonCode::EInvalidArgs),
        RunErrorKind::ProviderRateLimit => Some(ReasonCode::ERateLimit),
        RunErrorKind::ProviderTimeout => Some(ReasonCode::ETimeout),
        RunErrorKind::ProviderServer => Some(ReasonCode::EProvider5xx),
        RunErrorKind::Network => Some(ReasonCode::ENetworkError),
        RunErrorKind::JudgeUnavailable => Some(ReasonCode::EJudgeUnavailable),
        RunErrorKind::Other => None,
    }
}

pub(crate) fn reason_code_from_anyhow_error(err: &anyhow::Error) -> Option<ReasonCode> {
    reason_code_from_error_message(&assay_core::errors::RunError::from_anyhow(err).message)
}

pub(crate) fn decide_run_outcome(
    results: &[assay_core::model::TestResultRow],
    strict: bool,
    version: crate::exit_codes::ExitCodeVersion,
) -> crate::exit_codes::RunOutcome {
    use assay_core::model::TestStatus;

    // Helper to ensure exit code matches requested version
    let make_outcome = |reason: ReasonCode, msg: Option<String>, context: Option<&str>| {
        let mut o = RunOutcome::from_reason(reason, msg, context);
        o.exit_code = reason.exit_code_for(version);
        o
    };

    // Priority 1: Config/Argument Errors (Exit 2)
    for r in results {
        if let Some(reason) = reason_code_from_error_message(&r.message) {
            if matches!(
                reason,
                ReasonCode::ETraceNotFound
                    | ReasonCode::EMissingConfig
                    | ReasonCode::ECfgParse
                    | ReasonCode::EInvalidArgs
            ) {
                return make_outcome(reason, Some(r.message.clone()), None);
            }
        }
    }

    // Priority 2: Infrastructure Failures (Refined Heuristics)
    let infra_errors: Vec<&assay_core::model::TestResultRow> = results
        .iter()
        .filter(|r| matches!(r.status, TestStatus::Error))
        .collect();

    if !infra_errors.is_empty() {
        let reason = pick_infra_reason(&infra_errors);
        return make_outcome(
            reason,
            Some("Infrastructure failures detected".into()),
            None,
        );
    }

    // Priority 3: Judge uncertain (abstain) — exit 1, E_JUDGE_UNCERTAIN
    let abstain_count = results
        .iter()
        .filter(|r| has_judge_verdict_abstain(&r.details))
        .count();
    if abstain_count > 0 {
        let mut o = RunOutcome::judge_uncertain(abstain_count);
        o.exit_code = ReasonCode::EJudgeUncertain.exit_code_for(version);
        return o;
    }

    // Priority 4: Test Failures
    let fails = results
        .iter()
        .filter(|r| matches!(r.status, TestStatus::Fail))
        .count();
    if fails > 0 {
        let mut o = RunOutcome::test_failure(fails);
        o.exit_code = ReasonCode::ETestFailed.exit_code_for(version);
        return o;
    }

    // Priority 5: Strict Mode Violations
    if strict {
        let violations = results
            .iter()
            .filter(|r| {
                matches!(
                    r.status,
                    TestStatus::Warn | TestStatus::Flaky | TestStatus::Unstable
                )
            })
            .count();
        if violations > 0 {
            return make_outcome(
                ReasonCode::EPolicyViolation,
                Some(format!("Strict mode: {} policy violations", violations)),
                None,
            );
        }
    }

    // Priority 6: Success (ensure version compliance though Success is usually 0 in all versions)
    let mut o = RunOutcome::success();
    o.exit_code = ReasonCode::Success.exit_code_for(version);
    o
}

/// True if this result row has any judge metric with verdict "Abstain" (E7.5).
pub(crate) fn has_judge_verdict_abstain(details: &serde_json::Value) -> bool {
    let Some(metrics) = details.get("metrics").and_then(|m| m.as_object()) else {
        return false;
    };
    for (_name, metric_val) in metrics {
        if let Some(inner) = metric_val.get("details").and_then(|d| d.get("verdict")) {
            if inner.as_str() == Some("Abstain") {
                return true;
            }
        }
    }
    false
}

fn pick_infra_reason(
    errors: &[&assay_core::model::TestResultRow],
) -> crate::exit_codes::ReasonCode {
    for r in errors {
        if let Some(reason) = reason_code_from_error_message(&r.message) {
            if matches!(
                reason,
                ReasonCode::ERateLimit
                    | ReasonCode::ETimeout
                    | ReasonCode::EProvider5xx
                    | ReasonCode::ENetworkError
                    | ReasonCode::EJudgeUnavailable
            ) {
                return reason;
            }
        }
    }
    ReasonCode::EJudgeUnavailable
}

/// Build a Summary from RunOutcome for writing summary.json (same dir as run.json).
pub(crate) fn summary_from_outcome(
    outcome: &crate::exit_codes::RunOutcome,
    verify_enabled: bool,
) -> assay_core::report::summary::Summary {
    let assay_version = env!("CARGO_PKG_VERSION");
    if outcome.exit_code == 0 {
        assay_core::report::summary::Summary::success(assay_version, verify_enabled)
    } else {
        assay_core::report::summary::Summary::failure(
            outcome.exit_code,
            &outcome.reason_code,
            outcome.message.as_deref().unwrap_or(""),
            outcome.next_step.as_deref().unwrap_or(""),
            assay_version,
            verify_enabled,
        )
    }
}

pub(crate) fn write_extended_run_json(
    artifacts: &assay_core::report::RunArtifacts,
    outcome: &crate::exit_codes::RunOutcome,
    path: &PathBuf,
    sarif_omitted: Option<u64>,
) -> anyhow::Result<()> {
    // Manually construct the JSON to inject outcome fields
    let mut v = serde_json::to_value(artifacts)?;
    if let Some(obj) = v.as_object_mut() {
        // Inject top-level outcome fields for machine-readability (Canonical Contract)
        obj.insert(
            "exit_code".to_string(),
            serde_json::json!(outcome.exit_code),
        );
        obj.insert(
            "reason_code".to_string(),
            serde_json::json!(outcome.reason_code),
        );
        obj.insert(
            "reason_code_version".to_string(),
            serde_json::json!(assay_core::report::summary::REASON_CODE_VERSION),
        );

        // E7.2: seeds always present; order_seed/judge_seed as string or null (SPEC: avoid JSON number precision loss)
        obj.insert(
            "seed_version".to_string(),
            serde_json::json!(assay_core::report::summary::SEED_VERSION),
        );
        let order_seed_json = match artifacts.order_seed {
            Some(n) => serde_json::Value::String(n.to_string()),
            None => serde_json::Value::Null,
        };
        obj.insert("order_seed".to_string(), order_seed_json);
        obj.insert("judge_seed".to_string(), serde_json::Value::Null);

        // E7.3: judge metrics when present
        if let Some(metrics) =
            assay_core::report::summary::judge_metrics_from_results(&artifacts.results)
        {
            obj.insert("judge_metrics".to_string(), serde_json::to_value(metrics)?);
        }

        // E2.3: SARIF truncation metadata when SARIF was truncated
        if let Some(n) = sarif_omitted {
            if n > 0 {
                obj.insert("sarif".to_string(), serde_json::json!({ "omitted": n }));
            }
        }

        // Conflict avoidance: Move full details to 'resolution' object
        // Do NOT inject 'message' or 'next_step' top-level to avoid collisions with artifact fields.
        obj.insert("resolution".to_string(), serde_json::to_value(outcome)?);

        if !outcome.warnings.is_empty() {
            obj.insert("warnings".into(), serde_json::json!(outcome.warnings));
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&v)?)?;
    Ok(())
}

pub(crate) fn write_run_json_minimal(
    outcome: &crate::exit_codes::RunOutcome,
    path: &PathBuf,
) -> anyhow::Result<()> {
    // Minimal JSON for early exits (no artifacts available). E7.2: seed fields present for schema stability (null when unknown).
    let v = serde_json::json!({
        "exit_code": outcome.exit_code,
        "reason_code": outcome.reason_code,
        "reason_code_version": assay_core::report::summary::REASON_CODE_VERSION,
        "seed_version": assay_core::report::summary::SEED_VERSION,
        "order_seed": null,
        "judge_seed": null,
        "resolution": outcome
    });
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&v)?)?;
    Ok(())
}

pub(crate) fn export_baseline(
    path: &PathBuf,
    config_path: &Path,
    cfg: &assay_core::model::EvalConfig,
    results: &[assay_core::model::TestResultRow],
) -> anyhow::Result<()> {
    let mut entries = Vec::new();

    for r in results {
        if let Some(metrics) = r.details.get("metrics").and_then(|v| v.as_object()) {
            for (metric_name, m_val) in metrics {
                if let Some(score) = m_val.get("score").and_then(|s| s.as_f64()) {
                    entries.push(assay_core::baseline::BaselineEntry {
                        test_id: r.test_id.clone(),
                        metric: metric_name.clone(),
                        score,
                        meta: None,
                    });
                }
            }
        }
    }

    let b = assay_core::baseline::Baseline {
        schema_version: 1,
        suite: cfg.suite.clone(),
        assay_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        config_fingerprint: assay_core::baseline::compute_config_fingerprint(config_path),
        git_info: None,
        entries,
    };

    b.save(path)?;
    eprintln!("exported baseline to {}", path.display());
    Ok(())
}

#[cfg(test)]
mod run_outcome_tests {
    use super::{decide_run_outcome, has_judge_verdict_abstain, reason_code_from_error_message};
    use crate::exit_codes::{ExitCodeVersion, ReasonCode, EXIT_INFRA_ERROR};
    use assay_core::model::{TestResultRow, TestStatus};

    #[test]
    fn test_infra_beats_abstain_precedence() {
        // Precedence: infra (exit 3) must win over abstain (exit 1). One Error + one Abstain → exit 3.
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
}
