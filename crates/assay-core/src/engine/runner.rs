use crate::attempts::{classify_attempts, FailureClass};
use crate::cache::key::cache_key;
use crate::cache::vcr::VcrCache;
use crate::errors::try_map_error;
use crate::metrics_api::Metric;
use crate::model::{AttemptRow, EvalConfig, LlmResponse, TestCase, TestResultRow, TestStatus};
use crate::providers::llm::LlmClient;
use crate::quarantine::{QuarantineMode, QuarantineService};
use crate::report::RunArtifacts;
use crate::storage::store::Store;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{timeout, Duration};

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
    pub embedder: Option<Arc<dyn crate::providers::embedder::Embedder>>,
    pub refresh_embeddings: bool,
    pub incremental: bool,
    pub refresh_cache: bool,
    pub judge: Option<crate::judge::JudgeService>,
    pub baseline: Option<crate::baseline::Baseline>,
}

impl Runner {
    pub async fn run_suite(&self, cfg: &EvalConfig) -> anyhow::Result<RunArtifacts> {
        let run_id = self.store.create_run(cfg)?;

        let parallel = cfg.settings.parallel.unwrap_or(4).max(1);
        let sem = Arc::new(Semaphore::new(parallel));
        let mut handles = Vec::new();

        for tc in cfg.tests.iter() {
            let permit = sem.clone().acquire_owned().await?;
            let this = self.clone_for_task();
            let cfg = cfg.clone();
            let tc = tc.clone();
            let h = tokio::spawn(async move {
                let _permit = permit;
                this.run_test_with_policy(&cfg, &tc, run_id).await
            });
            handles.push(h);
        }

        let mut rows = Vec::new();
        let mut any_fail = false;
        for h in handles {
            let row = match h.await {
                Ok(Ok(row)) => row,
                Ok(Err(e)) => TestResultRow {
                    test_id: "unknown".into(),
                    status: TestStatus::Error,
                    score: None,
                    cached: false,
                    message: format!("task error: {}", e),
                    details: serde_json::json!({}),
                    duration_ms: None,
                    fingerprint: None,
                    skip_reason: None,
                    attempts: None,
                },
                Err(e) => TestResultRow {
                    test_id: "unknown".into(),
                    status: TestStatus::Error,
                    score: None,
                    cached: false,
                    message: format!("join error: {}", e),
                    details: serde_json::json!({}),
                    duration_ms: None,
                    fingerprint: None,
                    skip_reason: None,
                    attempts: None,
                },
            };
            any_fail = any_fail || matches!(row.status, TestStatus::Fail | TestStatus::Error);
            rows.push(row);
        }

        self.store
            .finalize_run(run_id, if any_fail { "failed" } else { "passed" })?;
        Ok(RunArtifacts {
            run_id,
            suite: cfg.suite.clone(),
            results: rows,
        })
    }

    async fn run_test_with_policy(
        &self,
        cfg: &EvalConfig,
        tc: &TestCase,
        run_id: i64,
    ) -> anyhow::Result<TestResultRow> {
        let quarantine = QuarantineService::new(self.store.clone());
        let q_reason = quarantine.is_quarantined(&cfg.suite, &tc.id)?;

        let max_attempts = 1 + self.policy.rerun_failures;
        let mut attempts: Vec<AttemptRow> = Vec::new();
        let mut last_row: Option<TestResultRow> = None;
        let mut last_output: Option<LlmResponse> = None;

        for i in 0..max_attempts {
            // Catch execution errors and convert to ResultRow to leverage retry/reporting logic
            let (row, output) = match self.run_test_once(cfg, tc).await {
                Ok(res) => res,
                Err(e) => {
                    let msg = if let Some(diag) = try_map_error(&e) {
                        diag.to_string()
                    } else {
                        e.to_string()
                    };

                    (
                        TestResultRow {
                            test_id: tc.id.clone(),
                            status: TestStatus::Error,
                            score: None,
                            cached: false,
                            message: msg,
                            details: serde_json::json!({ "error": true }),
                            duration_ms: None,
                            fingerprint: None,
                            skip_reason: None,
                            attempts: None,
                        },
                        LlmResponse {
                            text: "".into(),
                            provider: "error".into(),
                            model: cfg.model.clone(),
                            cached: false,
                            meta: serde_json::json!({}),
                        },
                    )
                }
            };
            attempts.push(AttemptRow {
                attempt_no: i + 1,
                status: row.status.clone(),
                message: row.message.clone(),
                duration_ms: row.duration_ms,
                details: row.details.clone(),
            });
            last_row = Some(row.clone());
            last_output = Some(output.clone());

            match row.status {
                TestStatus::Pass | TestStatus::Warn => break,
                TestStatus::Skipped => break, // Should not happen in loop
                TestStatus::Fail | TestStatus::Error | TestStatus::Flaky | TestStatus::Unstable => {
                    continue
                }
            }
        }

        let class = classify_attempts(&attempts);
        let mut final_row = last_row.unwrap_or(TestResultRow {
            test_id: tc.id.clone(),
            status: TestStatus::Error,
            score: None,
            cached: false,
            message: "no attempts".into(),
            details: serde_json::json!({}),
            duration_ms: None,
            fingerprint: None,
            skip_reason: None,
            attempts: None,
        });

        // quarantine overlay
        if let Some(reason) = q_reason {
            match self.policy.quarantine_mode {
                QuarantineMode::Off => {}
                QuarantineMode::Warn => {
                    final_row.status = TestStatus::Warn;
                    final_row.message = format!("quarantined: {}", reason);
                }
                QuarantineMode::Strict => {
                    final_row.status = TestStatus::Fail;
                    final_row.message = format!("quarantined (strict): {}", reason);
                }
            }
        }

        match class {
            FailureClass::Skipped => {
                final_row.status = TestStatus::Skipped;
                // message usually set by run_test_once
            }
            FailureClass::Flaky => {
                final_row.status = TestStatus::Flaky;
                final_row.message = "flake detected (rerun passed)".into();
                final_row.details["flake"] = serde_json::json!({ "attempts": attempts.len() });
            }
            FailureClass::Unstable => {
                final_row.status = TestStatus::Unstable;
                final_row.message = "unstable outcomes detected".into();
                final_row.details["unstable"] = serde_json::json!({ "attempts": attempts.len() });
            }
            FailureClass::Error => final_row.status = TestStatus::Error,
            FailureClass::DeterministicFail => {
                // Ensures if last attempt was fail, we keep fail status
                final_row.status = TestStatus::Fail;
            }
            FailureClass::DeterministicPass => {
                final_row.status = TestStatus::Pass;
            }
        }

        let output = last_output.unwrap_or(LlmResponse {
            text: "".into(),
            provider: self.client.provider_name().to_string(),
            model: cfg.model.clone(),
            cached: false,
            meta: serde_json::json!({}),
        });

        final_row.attempts = Some(attempts.clone());

        // PR-4.0.3 Agent Assertions
        if let Some(assertions) = &tc.assertions {
            if !assertions.is_empty() {
                // Verify assertions against DB
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

                            // serialize diagnostics
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

        self.store
            .insert_result_embedded(run_id, &final_row, &attempts, &output)?;

        Ok(final_row)
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

        // Incremental Check
        // Note: Global --incremental flag should be checked here.
        // Assuming self.incremental is available.
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
        self.enrich_semantic(tc, &mut resp).await?;
        self.enrich_judge(tc, &mut resp).await?;

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
        };

        if self.client.provider_name() == "trace" {
            row.details["assay.replay"] = serde_json::json!(true);
        }

        row.details["prompt"] = serde_json::Value::String(tc.input.prompt.clone());

        Ok((row, resp))
    }

    async fn call_llm(&self, cfg: &EvalConfig, tc: &TestCase) -> anyhow::Result<LlmResponse> {
        let t = cfg.settings.timeout_seconds.unwrap_or(30);
        let fut = self
            .client
            .complete(&tc.input.prompt, tc.input.context.as_deref());
        let resp = timeout(Duration::from_secs(t), fut).await??;
        Ok(resp)
    }

    fn clone_for_task(&self) -> RunnerRef {
        RunnerRef {
            store: self.store.clone(),
            cache: self.cache.clone(),
            client: self.client.clone(),
            metrics: self.metrics.clone(),
            policy: self.policy.clone(),
            embedder: self.embedder.clone(),
            refresh_embeddings: self.refresh_embeddings,
            incremental: self.incremental,
            refresh_cache: self.refresh_cache,
            judge: self.judge.clone(),
            baseline: self.baseline.clone(),
        }
    }

    fn check_baseline_regressions(
        &self,
        tc: &TestCase,
        cfg: &EvalConfig,
        details: &serde_json::Value,
        metrics: &[Arc<dyn Metric>],
        baseline: &crate::baseline::Baseline,
    ) -> Option<(TestStatus, String)> {
        // Check suite-level defaults
        let suite_defaults = cfg.settings.thresholding.as_ref();

        for m in metrics {
            let metric_name = m.name();
            // Only numeric metrics supported right now
            let score = details["metrics"][metric_name]["score"].as_f64()?;

            // Determine thresholding config
            // 1. Metric override (from expected enum - tricky as Metric trait hides this)
            // Use suite defaults unless specific metric logic overrides
            // Actually, `tc.expected` has the config. We need to parse it.

            let (mode, max_drop) = self.resolve_threshold_config(tc, metric_name, suite_defaults);

            if mode == "relative" {
                if let Some(base_score) = baseline.get_score(&tc.id, metric_name) {
                    let delta = score - base_score;
                    if let Some(drop_limit) = max_drop {
                        if delta < -drop_limit {
                            return Some((
                                TestStatus::Fail,
                                format!(
                                    "regression: {} dropped {:.3} (limit: {:.3})",
                                    metric_name, -delta, drop_limit
                                ),
                            ));
                        }
                    }
                } else {
                    // Missing baseline
                    return Some((
                        TestStatus::Warn,
                        format!("missing baseline for {}/{}", tc.id, metric_name),
                    ));
                }
            }
        }
        None
    }

    fn resolve_threshold_config(
        &self,
        _tc: &TestCase,
        _metric_name: &str,
        suite_defaults: Option<&crate::model::ThresholdingSettings>,
    ) -> (String, Option<f64>) {
        // Defaults
        let mut mode = "absolute".to_string();
        let mut max_drop = None;

        if let Some(s) = suite_defaults {
            if let Some(m) = &s.mode {
                mode = m.clone();
            }
            max_drop = s.max_drop;
        }

        // Per-metric overrides
        // Provide a clumsy match here or better implement a helper on Expected
        // For MVP, we'll check suite defaults primarily.
        // Actual implementation requires mapping metric_name to Expected variant fields.
        // Let's stick to suite defaults for this iteration to get it compiling.
        (mode, max_drop)
    }

    // Embeddings logic
    async fn enrich_semantic(&self, tc: &TestCase, resp: &mut LlmResponse) -> anyhow::Result<()> {
        use crate::model::Expected;

        let Expected::SemanticSimilarityTo {
            semantic_similarity_to,
            ..
        } = &tc.expected
        else {
            return Ok(());
        };

        if resp.meta.pointer("/assay/embeddings/response").is_some()
            && resp.meta.pointer("/assay/embeddings/reference").is_some()
        {
            return Ok(());
        }

        if self.policy.replay_strict {
            anyhow::bail!("config error: --replay-strict is on, but embeddings are missing in trace. Run 'assay trace precompute-embeddings' or disable strict mode.");
        }

        let embedder = self.embedder.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "config error: semantic_similarity_to requires an embedder (--embedder) or trace meta embeddings"
            )
        })?;

        let model_id = embedder.model_id();

        let (resp_vec, src_resp) = self
            .embed_text(&model_id, embedder.as_ref(), &resp.text)
            .await?;
        let (ref_vec, src_ref) = self
            .embed_text(&model_id, embedder.as_ref(), semantic_similarity_to)
            .await?;

        // write into meta.assay.embeddings
        if !resp.meta.get("assay").is_some_and(|v| v.is_object()) {
            resp.meta["assay"] = serde_json::json!({});
        }
        resp.meta["assay"]["embeddings"] = serde_json::json!({
            "model": model_id,
            "response": resp_vec,
            "reference": ref_vec,
            "source_response": src_resp,
            "source_reference": src_ref
        });

        Ok(())
    }

    pub async fn embed_text(
        &self,
        model_id: &str,
        embedder: &dyn crate::providers::embedder::Embedder,
        text: &str,
    ) -> anyhow::Result<(Vec<f32>, &'static str)> {
        use crate::embeddings::util::embed_cache_key;

        let key = embed_cache_key(model_id, text);

        if !self.refresh_embeddings {
            if let Some((_m, vec)) = self.store.get_embedding(&key)? {
                return Ok((vec, "cache"));
            }
        }

        let vec = embedder.embed(text).await?;
        self.store.put_embedding(&key, model_id, &vec)?;
        Ok((vec, "live"))
    }

    async fn enrich_judge(&self, tc: &TestCase, resp: &mut LlmResponse) -> anyhow::Result<()> {
        use crate::model::Expected;

        let (rubric_id, rubric_version) = match &tc.expected {
            Expected::Faithfulness { rubric_version, .. } => {
                ("faithfulness", rubric_version.as_deref())
            }
            Expected::Relevance { rubric_version, .. } => ("relevance", rubric_version.as_deref()),
            _ => return Ok(()),
        };

        // Check if judge result exists in meta is handled by JudgeService::evaluate
        // BUT for a better error message in strict mode we can check here too or rely on the StrictLlmClient failure.
        // User requested: "judge guard ... missing judge result in trace meta ... run precompute-judge"

        let has_trace = resp
            .meta
            .pointer(&format!("/assay/judge/{}", rubric_id))
            .is_some();
        if self.policy.replay_strict && !has_trace {
            anyhow::bail!("config error: --replay-strict is on, but judge results are missing in trace for '{}'. Run 'assay trace precompute-judge' or disable strict mode.", rubric_id);
        }

        let judge = self.judge.as_ref().ok_or_else(|| {
            anyhow::anyhow!("config error: judge required but service not initialized")
        })?;

        judge
            .evaluate(
                &tc.id,
                rubric_id,
                &tc.input,
                &resp.text,
                rubric_version,
                &mut resp.meta,
            )
            .await?;

        Ok(())
    }
}

#[derive(Clone)]
struct RunnerRef {
    store: Store,
    cache: VcrCache,
    client: Arc<dyn LlmClient>,
    metrics: Vec<Arc<dyn Metric>>,
    policy: RunPolicy,
    embedder: Option<Arc<dyn crate::providers::embedder::Embedder>>,
    refresh_embeddings: bool,
    incremental: bool,
    refresh_cache: bool,
    judge: Option<crate::judge::JudgeService>,
    baseline: Option<crate::baseline::Baseline>,
}

impl RunnerRef {
    async fn run_test_with_policy(
        &self,
        cfg: &EvalConfig,
        tc: &TestCase,
        run_id: i64,
    ) -> anyhow::Result<TestResultRow> {
        let runner = Runner {
            store: self.store.clone(),
            cache: self.cache.clone(),
            client: self.client.clone(),
            metrics: self.metrics.clone(),
            policy: self.policy.clone(),
            embedder: self.embedder.clone(),
            refresh_embeddings: self.refresh_embeddings,
            incremental: self.incremental,
            refresh_cache: self.refresh_cache,
            judge: self.judge.clone(),
            baseline: self.baseline.clone(),
        };
        runner.run_test_with_policy(cfg, tc, run_id).await
    }
}
