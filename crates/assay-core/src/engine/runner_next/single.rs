use super::super::Runner;
use crate::cache::key::cache_key;
use crate::model::{EvalConfig, LlmResponse, TestCase, TestResultRow, TestStatus};
use tracing::{info_span, Instrument};

pub(crate) async fn run_test_once_impl(
    runner: &Runner,
    cfg: &EvalConfig,
    tc: &TestCase,
) -> anyhow::Result<(TestResultRow, LlmResponse)> {
    let expected_json = serde_json::to_string(&tc.expected).unwrap_or_default();
    let metric_versions = [("assay", env!("CARGO_PKG_VERSION"))];

    let policy_hash = if let Some(path) = tc.expected.get_policy_path() {
        match std::fs::read_to_string(path) {
            Ok(content) => Some(crate::fingerprint::sha256_hex(&content)),
            Err(_) => None,
        }
    } else {
        None
    };

    let fp = crate::fingerprint::compute(crate::fingerprint::Context {
        suite: &cfg.suite,
        model: &cfg.model,
        test_id: &tc.id,
        prompt: &tc.input.prompt,
        context: tc.input.context.as_deref(),
        expected_canonical: &expected_json,
        policy_hash: policy_hash.as_deref(),
        metric_versions: &metric_versions,
    });

    if runner.incremental && !runner.refresh_cache {
        if let Some(prev) = runner.store.get_last_passing_by_fingerprint(&fp.hex)? {
            let row = TestResultRow {
                test_id: tc.id.clone(),
                status: TestStatus::Skipped,
                score: prev.score,
                cached: true,
                message: "skipped: fingerprint match".into(),
                details: serde_json::json!({
                    "skip": {
                         "reason": "fingerprint_match",
                         "fingerprint": fp.hex,
                         "previous_run_id": prev.details.get("skip").and_then(|s: &serde_json::Value| s.get("previous_run_id")).and_then(|v: &serde_json::Value| v.as_i64()),
                         "previous_at": prev.details.get("skip").and_then(|s: &serde_json::Value| s.get("previous_at")).and_then(|v: &serde_json::Value| v.as_str()),
                         "origin_run_id": prev.details.get("skip").and_then(|s: &serde_json::Value| s.get("origin_run_id")).and_then(|v: &serde_json::Value| v.as_i64()),
                         "previous_score": prev.score
                    }
                }),
                duration_ms: Some(0),
                fingerprint: Some(fp.hex.clone()),
                skip_reason: Some("fingerprint_match".into()),
                attempts: None,
                error_policy_applied: None,
            };

            let resp = LlmResponse {
                text: "".into(),
                provider: "skipped".into(),
                model: cfg.model.clone(),
                cached: true,
                meta: serde_json::json!({}),
            };
            return Ok((row, resp));
        }
    }

    let key = cache_key(
        &cfg.model,
        &tc.input.prompt,
        &fp.hex,
        runner.client.fingerprint().as_deref(),
    );

    let start = std::time::Instant::now();
    let mut cached = false;

    let mut resp: LlmResponse = if cfg.settings.cache.unwrap_or(true) && !runner.refresh_cache {
        if let Some(r) = runner.cache.get(&key)? {
            cached = true;
            eprintln!(
                "  [CACHE HIT] key={} prompt_len={}",
                key,
                tc.input.prompt.len()
            );
            r
        } else {
            let r = runner.call_llm(cfg, tc).await?;
            runner.cache.put(&key, &r)?;
            r
        }
    } else {
        runner.call_llm(cfg, tc).await?
    };
    resp.cached = resp.cached || cached;

    runner.enrich_semantic(cfg, tc, &mut resp).await?;
    runner.enrich_judge(cfg, tc, &mut resp).await?;

    let mut final_status = TestStatus::Pass;
    let mut final_score: Option<f64> = None;
    let mut msg = String::new();
    let mut details = serde_json::json!({ "metrics": {} });

    for m in &runner.metrics {
        let metric_name = m.name();
        let metric_span = info_span!(
            "assay.eval.metric",
            "assay.eval.test_id" = tc.id.as_str(),
            "assay.eval.metric.name" = metric_name,
            "assay.eval.response.cached" = resp.cached,
            "assay.eval.metric.score" = tracing::field::Empty,
            "assay.eval.metric.passed" = tracing::field::Empty,
            "assay.eval.metric.unstable" = tracing::field::Empty,
            "assay.eval.metric.duration_ms" = tracing::field::Empty,
            "error" = tracing::field::Empty,
            "error.message" = tracing::field::Empty
        );
        let metric_start = std::time::Instant::now();
        let metric_result = async { m.evaluate(tc, &tc.expected, &resp).await }
            .instrument(metric_span.clone())
            .await;
        let metric_duration_ms = metric_start.elapsed().as_millis() as u64;
        metric_span.record("assay.eval.metric.duration_ms", metric_duration_ms);

        let r = match metric_result {
            Ok(result) => {
                metric_span.record("assay.eval.metric.score", result.score);
                metric_span.record("assay.eval.metric.passed", result.passed);
                metric_span.record("assay.eval.metric.unstable", result.unstable);
                result
            }
            Err(err) => {
                let error_message = err.to_string();
                metric_span.record("error", true);
                metric_span.record("error.message", error_message.as_str());
                return Err(err);
            }
        };

        details["metrics"][metric_name] = serde_json::json!({
            "score": r.score, "passed": r.passed, "unstable": r.unstable, "details": r.details
        });
        final_score = Some(r.score);

        if r.unstable {
            final_status = TestStatus::Warn;
            msg = format!("unstable metric: {}", metric_name);
            break;
        }
        if !r.passed {
            final_status = TestStatus::Fail;
            msg = format!("failed: {}", metric_name);
            break;
        }
    }

    if let Some(baseline) = &runner.baseline {
        if let Some((new_status, new_msg)) =
            runner.check_baseline_regressions(tc, cfg, &details, &runner.metrics, baseline)
        {
            if matches!(new_status, TestStatus::Fail | TestStatus::Warn) {
                final_status = new_status;
                msg = new_msg;
            }
        }
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    let mut row = TestResultRow {
        test_id: tc.id.clone(),
        status: final_status,
        score: final_score,
        cached: resp.cached,
        message: if msg.is_empty() { "ok".into() } else { msg },
        details,
        duration_ms: Some(duration_ms),
        fingerprint: Some(fp.hex),
        skip_reason: None,
        attempts: None,
        error_policy_applied: None,
    };

    if runner.client.provider_name() == "trace" {
        row.details["assay.replay"] = serde_json::json!(true);
    }

    row.details["prompt"] = serde_json::Value::String(tc.input.prompt.clone());

    Ok((row, resp))
}
