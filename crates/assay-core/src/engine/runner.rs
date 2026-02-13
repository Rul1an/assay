use crate::cache::key::cache_key;
use crate::cache::vcr::VcrCache;
use crate::metrics_api::Metric;
use crate::model::{EvalConfig, LlmResponse, TestCase, TestResultRow, TestStatus};
use crate::providers::llm::LlmClient;
use crate::quarantine::QuarantineMode;
use crate::report::progress::ProgressSink;
use crate::report::RunArtifacts;
use crate::storage::store::Store;
use std::sync::Arc;

#[path = "runner_next/mod.rs"]
mod runner_next;

#[derive(Debug, Clone)]
pub struct RunPolicy {
    pub rerun_failures: u32,
    pub quarantine_mode: QuarantineMode,
    pub replay_strict: bool,
}

impl Default for RunPolicy {
    fn default() -> Self {
        Self {
            rerun_failures: 1,
            quarantine_mode: QuarantineMode::Warn,
            replay_strict: false,
        }
    }
}

pub struct Runner {
    pub store: Store,
    pub cache: VcrCache,
    pub client: Arc<dyn LlmClient>,
    pub metrics: Vec<Arc<dyn Metric>>,
    pub policy: RunPolicy,
    pub _network_guard: Option<crate::providers::network::NetworkPolicyGuard>,
    pub embedder: Option<Arc<dyn crate::providers::embedder::Embedder>>,
    pub refresh_embeddings: bool,
    pub incremental: bool,
    pub refresh_cache: bool,
    pub judge: Option<crate::judge::JudgeService>,
    pub baseline: Option<crate::baseline::Baseline>,
}

impl Runner {
    /// Run the suite; results are collected in completion order internally but returned
    /// sorted by test_id for deterministic output. If `progress` is set, it is called
    /// after each test completes (E4.3 realtime progress).
    pub async fn run_suite(
        &self,
        cfg: &EvalConfig,
        progress: Option<ProgressSink>,
    ) -> anyhow::Result<RunArtifacts> {
        runner_next::execute::run_suite_impl(self, cfg, progress).await
    }

    fn apply_agent_assertions(
        &self,
        run_id: i64,
        tc: &TestCase,
        final_row: &mut TestResultRow,
    ) -> anyhow::Result<()> {
        if let Some(assertions) = &tc.assertions {
            if !assertions.is_empty() {
                match crate::agent_assertions::verify_assertions(
                    &self.store,
                    run_id,
                    &tc.id,
                    assertions,
                ) {
                    Ok(diags) => {
                        if !diags.is_empty() {
                            // Assertion Failures
                            final_row.status = TestStatus::Fail;

                            let diag_json: Vec<serde_json::Value> = diags
                                .iter()
                                .map(|d| serde_json::to_value(d).unwrap_or_default())
                                .collect();

                            final_row.details["assertions"] = serde_json::Value::Array(diag_json);

                            let fail_msg = format!("assertions failed ({})", diags.len());
                            if final_row.message == "ok" {
                                final_row.message = fail_msg;
                            } else {
                                final_row.message = format!("{}; {}", final_row.message, fail_msg);
                            }
                        } else {
                            // passed
                            final_row.details["assertions"] = serde_json::json!({ "passed": true });
                        }
                    }
                    Err(e) => {
                        // Missing or Ambiguous Episode -> Fail
                        final_row.status = TestStatus::Fail;
                        final_row.message = format!("assertions error: {}", e);
                        final_row.details["assertions"] =
                            serde_json::json!({ "error": e.to_string() });
                    }
                }
            }
        }
        Ok(())
    }

    async fn run_test_once(
        &self,
        cfg: &EvalConfig,
        tc: &TestCase,
    ) -> anyhow::Result<(TestResultRow, LlmResponse)> {
        let expected_json = serde_json::to_string(&tc.expected).unwrap_or_default();
        let metric_versions = [("assay", env!("CARGO_PKG_VERSION"))];

        let policy_hash = if let Some(path) = tc.expected.get_policy_path() {
            // Read policy content to ensure cache invalidation on content change
            match std::fs::read_to_string(path) {
                Ok(content) => Some(crate::fingerprint::sha256_hex(&content)),
                Err(_) => None, // If file missing, finding it later will error.
                                // We don't fail here to allow error reporting in metrics phase or main loop.
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

        // Incremental cache check.
        if self.incremental && !self.refresh_cache {
            if let Some(prev) = self.store.get_last_passing_by_fingerprint(&fp.hex)? {
                // Return Skipped Result
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
                    duration_ms: Some(0), // Instant
                    fingerprint: Some(fp.hex.clone()),
                    skip_reason: Some("fingerprint_match".into()),
                    attempts: None,
                    error_policy_applied: None,
                };

                // Construct placeholder response for pipeline consistency
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

        // Original Execution Logic
        // We use the computed fingerprint for caching key to distinguish config variations
        let key = cache_key(
            &cfg.model,
            &tc.input.prompt,
            &fp.hex,
            self.client.fingerprint().as_deref(),
        );

        let start = std::time::Instant::now();
        let mut cached = false;

        let mut resp: LlmResponse = if cfg.settings.cache.unwrap_or(true) && !self.refresh_cache {
            if let Some(r) = self.cache.get(&key)? {
                cached = true;
                eprintln!(
                    "  [CACHE HIT] key={} prompt_len={}",
                    key,
                    tc.input.prompt.len()
                );
                r
            } else {
                let r = self.call_llm(cfg, tc).await?;
                self.cache.put(&key, &r)?;
                r
            }
        } else {
            self.call_llm(cfg, tc).await?
        };
        resp.cached = resp.cached || cached;

        // Semantic Enrichment
        self.enrich_semantic(cfg, tc, &mut resp).await?;
        self.enrich_judge(cfg, tc, &mut resp).await?;

        let mut final_status = TestStatus::Pass;
        let mut final_score: Option<f64> = None;
        let mut msg = String::new();
        let mut details = serde_json::json!({ "metrics": {} });

        for m in &self.metrics {
            let r = m.evaluate(tc, &tc.expected, &resp).await?;
            details["metrics"][m.name()] = serde_json::json!({
                "score": r.score, "passed": r.passed, "unstable": r.unstable, "details": r.details
            });
            final_score = Some(r.score);

            if r.unstable {
                // gate stability first: treat unstable as warn in MVP
                final_status = TestStatus::Warn;
                msg = format!("unstable metric: {}", m.name());
                break;
            }
            if !r.passed {
                final_status = TestStatus::Fail;
                msg = format!("failed: {}", m.name());
                break;
            }
        }

        // Post-metric baseline check
        if let Some(baseline) = &self.baseline {
            if let Some((new_status, new_msg)) =
                self.check_baseline_regressions(tc, cfg, &details, &self.metrics, baseline)
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

        if self.client.provider_name() == "trace" {
            row.details["assay.replay"] = serde_json::json!(true);
        }

        row.details["prompt"] = serde_json::Value::String(tc.input.prompt.clone());

        Ok((row, resp))
    }

    async fn call_llm(&self, cfg: &EvalConfig, tc: &TestCase) -> anyhow::Result<LlmResponse> {
        runner_next::execute::call_llm_impl(self, cfg, tc).await
    }

    fn check_baseline_regressions(
        &self,
        tc: &TestCase,
        cfg: &EvalConfig,
        details: &serde_json::Value,
        metrics: &[Arc<dyn Metric>],
        baseline: &crate::baseline::Baseline,
    ) -> Option<(TestStatus, String)> {
        runner_next::baseline::check_baseline_regressions_impl(
            self, tc, cfg, details, metrics, baseline,
        )
    }

    // Embeddings logic
    async fn enrich_semantic(
        &self,
        _cfg: &EvalConfig,
        tc: &TestCase,
        resp: &mut LlmResponse,
    ) -> anyhow::Result<()> {
        runner_next::scoring::enrich_semantic_impl(self, _cfg, tc, resp).await
    }

    pub async fn embed_text(
        &self,
        model_id: &str,
        embedder: &dyn crate::providers::embedder::Embedder,
        text: &str,
    ) -> anyhow::Result<(Vec<f32>, &'static str)> {
        runner_next::cache::embed_text_impl(self, model_id, embedder, text).await
    }

    async fn enrich_judge(
        &self,
        cfg: &EvalConfig,
        tc: &TestCase,
        resp: &mut LlmResponse,
    ) -> anyhow::Result<()> {
        runner_next::scoring::enrich_judge_impl(self, cfg, tc, resp).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics_api::{Metric, MetricResult};
    use crate::model::{Expected, Settings, TestInput};
    use crate::on_error::ErrorPolicy;
    use crate::providers::llm::fake::FakeClient;
    use crate::providers::llm::LlmClient;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone, Copy)]
    enum MetricMode {
        FailThenPass,
        AlwaysFail,
        AlwaysPass,
    }

    struct ScriptedMetric {
        mode: MetricMode,
        calls: AtomicUsize,
    }

    impl ScriptedMetric {
        fn fail_then_pass() -> Self {
            Self {
                mode: MetricMode::FailThenPass,
                calls: AtomicUsize::new(0),
            }
        }

        fn always_fail() -> Self {
            Self {
                mode: MetricMode::AlwaysFail,
                calls: AtomicUsize::new(0),
            }
        }

        fn always_pass() -> Self {
            Self {
                mode: MetricMode::AlwaysPass,
                calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl Metric for ScriptedMetric {
        fn name(&self) -> &'static str {
            "scripted"
        }

        async fn evaluate(
            &self,
            _tc: &TestCase,
            _expected: &Expected,
            _resp: &LlmResponse,
        ) -> anyhow::Result<MetricResult> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            match self.mode {
                MetricMode::FailThenPass => {
                    if n == 0 {
                        Ok(MetricResult::fail(0.0, "scripted_fail_once"))
                    } else {
                        Ok(MetricResult::pass(1.0))
                    }
                }
                MetricMode::AlwaysFail => Ok(MetricResult::fail(0.0, "scripted_fail")),
                MetricMode::AlwaysPass => Ok(MetricResult::pass(1.0)),
            }
        }
    }

    struct ErrorClient;

    #[async_trait]
    impl LlmClient for ErrorClient {
        async fn complete(
            &self,
            _prompt: &str,
            _context: Option<&[String]>,
        ) -> anyhow::Result<LlmResponse> {
            Err(anyhow::anyhow!("scripted provider error"))
        }

        fn provider_name(&self) -> &'static str {
            "error_client"
        }
    }

    fn runner_for_contract_tests(
        client: Arc<dyn LlmClient>,
        metrics: Vec<Arc<dyn Metric>>,
        rerun_failures: u32,
    ) -> Runner {
        let store = Store::memory().expect("in-memory store");
        store.init_schema().expect("schema init");
        Runner {
            store: store.clone(),
            cache: VcrCache::new(store),
            client,
            metrics,
            policy: RunPolicy {
                rerun_failures,
                quarantine_mode: QuarantineMode::Off,
                replay_strict: false,
            },
            _network_guard: None,
            embedder: None,
            refresh_embeddings: false,
            incremental: false,
            refresh_cache: false,
            judge: None,
            baseline: None,
        }
    }

    fn single_test_config(on_error: ErrorPolicy) -> EvalConfig {
        EvalConfig {
            version: 1,
            suite: "runner-contract".to_string(),
            model: "fake-model".to_string(),
            settings: Settings {
                parallel: Some(1),
                cache: Some(false),
                seed: Some(1234),
                on_error,
                ..Default::default()
            },
            thresholds: Default::default(),
            otel: Default::default(),
            tests: vec![TestCase {
                id: "t1".to_string(),
                input: TestInput {
                    prompt: "contract prompt".to_string(),
                    context: None,
                },
                // Expected payload is not used by scripted metrics, but keeps test case valid.
                expected: Expected::MustContain {
                    must_contain: vec!["ok".to_string()],
                },
                assertions: None,
                on_error: None,
                tags: vec![],
                metadata: None,
            }],
        }
    }

    fn config_with_test_ids(ids: &[&str], on_error: ErrorPolicy) -> EvalConfig {
        EvalConfig {
            version: 1,
            suite: "runner-contract".to_string(),
            model: "fake-model".to_string(),
            settings: Settings {
                parallel: Some(1),
                cache: Some(false),
                seed: Some(1234),
                on_error,
                ..Default::default()
            },
            thresholds: Default::default(),
            otel: Default::default(),
            tests: ids
                .iter()
                .map(|id| TestCase {
                    id: (*id).to_string(),
                    input: TestInput {
                        prompt: format!("prompt-{id}"),
                        context: None,
                    },
                    expected: Expected::MustContain {
                        must_contain: vec!["ok".to_string()],
                    },
                    assertions: None,
                    on_error: None,
                    tags: vec![],
                    metadata: None,
                })
                .collect(),
        }
    }

    #[tokio::test]
    async fn runner_contract_flake_fail_then_pass_classified_flaky() -> anyhow::Result<()> {
        let cfg = single_test_config(ErrorPolicy::Block);
        let client = Arc::new(FakeClient::new("fake-model".to_string()).with_response("ok".into()));
        let metric = Arc::new(ScriptedMetric::fail_then_pass());
        let runner = runner_for_contract_tests(client, vec![metric], 1);

        let artifacts = runner.run_suite(&cfg, None).await?;
        let row = artifacts
            .results
            .iter()
            .find(|r| r.test_id == "t1")
            .expect("result for t1");

        assert_eq!(row.status, TestStatus::Flaky);
        assert_eq!(row.message, "flake detected (rerun passed)");
        let attempts = row.attempts.as_ref().expect("attempts");
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].status, TestStatus::Fail);
        assert_eq!(attempts[1].status, TestStatus::Pass);
        Ok(())
    }

    #[tokio::test]
    async fn runner_contract_fail_after_retries_stays_fail() -> anyhow::Result<()> {
        let cfg = single_test_config(ErrorPolicy::Block);
        let client = Arc::new(FakeClient::new("fake-model".to_string()).with_response("ok".into()));
        let metric = Arc::new(ScriptedMetric::always_fail());
        let runner = runner_for_contract_tests(client, vec![metric], 1);

        let artifacts = runner.run_suite(&cfg, None).await?;
        let row = artifacts
            .results
            .iter()
            .find(|r| r.test_id == "t1")
            .expect("result for t1");

        assert_eq!(row.status, TestStatus::Fail);
        assert!(
            row.message.contains("failed: scripted"),
            "expected stable failure reason, got: {}",
            row.message
        );
        let attempts = row.attempts.as_ref().expect("attempts");
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].status, TestStatus::Fail);
        assert_eq!(attempts[1].status, TestStatus::Fail);
        Ok(())
    }

    #[tokio::test]
    async fn runner_contract_on_error_allow_marks_allowed_and_policy_applied() -> anyhow::Result<()>
    {
        let cfg = single_test_config(ErrorPolicy::Allow);
        let client = Arc::new(ErrorClient);
        let runner = runner_for_contract_tests(client, vec![], 2);

        let artifacts = runner.run_suite(&cfg, None).await?;
        let row = artifacts
            .results
            .iter()
            .find(|r| r.test_id == "t1")
            .expect("result for t1");

        assert_eq!(row.status, TestStatus::AllowedOnError);
        assert_eq!(row.error_policy_applied, Some(ErrorPolicy::Allow));
        assert_eq!(row.details["policy_applied"], serde_json::json!("allow"));
        let attempts = row.attempts.as_ref().expect("attempts");
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].status, TestStatus::AllowedOnError);
        Ok(())
    }

    #[tokio::test]
    async fn runner_contract_results_sorted_by_test_id() -> anyhow::Result<()> {
        let mut cfg = config_with_test_ids(&["t3", "t1", "t2"], ErrorPolicy::Block);
        cfg.settings.parallel = Some(3);
        let client = Arc::new(FakeClient::new("fake-model".to_string()).with_response("ok".into()));
        let metric = Arc::new(ScriptedMetric::always_pass());
        let runner = runner_for_contract_tests(client, vec![metric], 0);

        let artifacts = runner.run_suite(&cfg, None).await?;
        let ids: Vec<_> = artifacts
            .results
            .iter()
            .map(|r| r.test_id.as_str())
            .collect();
        assert_eq!(ids, vec!["t1", "t2", "t3"]);
        Ok(())
    }

    #[tokio::test]
    async fn runner_contract_progress_sink_reports_done_total() -> anyhow::Result<()> {
        let cfg = config_with_test_ids(&["p1", "p2", "p3"], ErrorPolicy::Block);
        let client = Arc::new(FakeClient::new("fake-model".to_string()).with_response("ok".into()));
        let metric = Arc::new(ScriptedMetric::always_pass());
        let runner = runner_for_contract_tests(client, vec![metric], 0);

        let events = Arc::new(std::sync::Mutex::new(Vec::<(usize, usize)>::new()));
        let sink = {
            let events = Arc::clone(&events);
            Arc::new(move |ev: crate::report::progress::ProgressEvent| {
                events
                    .lock()
                    .expect("progress lock")
                    .push((ev.done, ev.total));
            }) as crate::report::progress::ProgressSink
        };

        let artifacts = runner.run_suite(&cfg, Some(sink)).await?;
        assert_eq!(artifacts.results.len(), 3);

        let observed = events.lock().expect("progress lock");
        assert_eq!(observed.len(), 3);
        assert_eq!(observed.last(), Some(&(3, 3)));
        assert!(observed.windows(2).all(|w| w[0].0 < w[1].0));
        Ok(())
    }

    #[tokio::test]
    async fn runner_contract_relative_baseline_missing_warns_in_helper() -> anyhow::Result<()> {
        let mut cfg = single_test_config(ErrorPolicy::Block);
        cfg.settings.thresholding = Some(crate::model::ThresholdingSettings {
            mode: Some("relative".to_string()),
            max_drop: Some(0.05),
            min_floor: None,
        });

        let client = Arc::new(FakeClient::new("fake-model".to_string()).with_response("ok".into()));
        let metric = Arc::new(ScriptedMetric::always_pass());
        let runner = runner_for_contract_tests(client, vec![], 0);
        let baseline = crate::baseline::Baseline {
            schema_version: 1,
            suite: "runner-contract".to_string(),
            assay_version: env!("CARGO_PKG_VERSION").to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            config_fingerprint: "md5:test".to_string(),
            git_info: None,
            entries: vec![],
        };
        let tc = cfg.tests.first().cloned().expect("single test case");
        let details = serde_json::json!({
            "metrics": {
                "scripted": {
                    "score": 1.0,
                    "passed": true,
                    "unstable": false,
                    "details": {}
                }
            }
        });

        let verdict = runner.check_baseline_regressions(&tc, &cfg, &details, &[metric], &baseline);
        let (status, message) = verdict.expect("relative baseline decision");
        assert_eq!(status, TestStatus::Warn);
        assert_eq!(message, "missing baseline for t1/scripted");
        Ok(())
    }
}
