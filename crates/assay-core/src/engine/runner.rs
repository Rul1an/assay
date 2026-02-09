use crate::attempts::{classify_attempts, FailureClass};
use crate::cache::key::cache_key;
use crate::cache::vcr::VcrCache;
use crate::errors::{try_map_error, RunError, RunErrorKind};
use crate::metrics_api::Metric;
use crate::model::{AttemptRow, EvalConfig, LlmResponse, TestCase, TestResultRow, TestStatus};
use crate::on_error::{ErrorPolicy, ErrorPolicyResult};
use crate::providers::llm::LlmClient;
use crate::quarantine::{QuarantineMode, QuarantineService};
use crate::report::progress::{ProgressEvent, ProgressSink};
use crate::report::RunArtifacts;
use crate::storage::store::Store;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
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
        let run_id = self.store.create_run(cfg)?;

        let parallel = cfg.settings.parallel.unwrap_or(4).max(1);
        let sem = Arc::new(Semaphore::new(parallel));
        let mut join_set = JoinSet::new();

        // E7.2: Randomized Order default (derived seed)
        // If seed is missing, generate one to ensure deterministic replay capability if logged
        // and to enforce default randomization.
        let mut cfg = cfg.clone();
        if cfg.settings.seed.is_none() {
            let s = rand::random();
            cfg.settings.seed = Some(s);
            // This ensures we always have a seed for 'run' artifacts and judge logic.
            eprintln!("Info: No seed provided. Using generated seed: {}", s);
        }

        let mut tests = cfg.tests.clone();
        if let Some(seed) = cfg.settings.seed {
            use rand::seq::SliceRandom;
            use rand::SeedableRng;
            // Use StdRng for reproducibility
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            tests.shuffle(&mut rng);
        }

        let total = tests.len();
        let mut clone_overhead_ms: u128 = 0;
        for tc in tests.iter() {
            let permit = sem.clone().acquire_owned().await?;
            let clone_started = Instant::now();
            let this = self.clone_for_task();
            clone_overhead_ms =
                clone_overhead_ms.saturating_add(clone_started.elapsed().as_millis());
            let cfg = cfg.clone();
            let tc = tc.clone();
            join_set.spawn(async move {
                let _permit = permit;
                this.run_test_with_policy(&cfg, &tc, run_id).await
            });
        }

        let mut rows = Vec::new();
        let mut any_fail = false;
        while let Some(res) = join_set.join_next().await {
            let row = match res {
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
                    error_policy_applied: None,
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
                    error_policy_applied: None,
                },
            };
            any_fail = any_fail || matches!(row.status, TestStatus::Fail | TestStatus::Error);
            rows.push(row);
            if total > 0 {
                if let Some(ref sink) = progress {
                    sink(ProgressEvent {
                        done: rows.len(),
                        total,
                    });
                }
            }
        }

        // Deterministic order for artifacts (replay, golden tests).
        rows.sort_by(|a, b| a.test_id.cmp(&b.test_id));

        self.store
            .finalize_run(run_id, if any_fail { "failed" } else { "passed" })?;
        Ok(RunArtifacts {
            run_id,
            suite: cfg.suite.clone(),
            results: rows,
            order_seed: cfg.settings.seed,
            runner_clone_ms: Some(clone_overhead_ms.min(u128::from(u64::MAX)) as u64),
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
        let error_policy = cfg.effective_error_policy(tc);

        let max_attempts = 1 + self.policy.rerun_failures;
        let mut attempts: Vec<AttemptRow> = Vec::new();
        let mut last_row: Option<TestResultRow> = None;
        let mut last_output: Option<LlmResponse> = None;

        for i in 0..max_attempts {
            let (row, output) = self.run_attempt_with_policy(cfg, tc, error_policy).await;
            Self::record_attempt(&mut attempts, i + 1, &row);
            last_row = Some(row.clone());
            last_output = Some(output.clone());

            if Self::should_stop_retries(row.status) {
                break;
            }
        }

        let class = classify_attempts(&attempts);
        let mut final_row = last_row.unwrap_or_else(|| Self::no_attempts_row(tc));
        self.apply_quarantine_overlay(&mut final_row, q_reason.as_deref());
        Self::apply_failure_classification(&mut final_row, class, attempts.len());

        let output = last_output.unwrap_or_else(|| self.empty_output_for_model(cfg));

        final_row.attempts = Some(attempts.clone());
        self.apply_agent_assertions(run_id, tc, &mut final_row)?;

        self.store
            .insert_result_embedded(run_id, &final_row, &attempts, &output)?;

        Ok(final_row)
    }

    async fn run_attempt_with_policy(
        &self,
        cfg: &EvalConfig,
        tc: &TestCase,
        error_policy: ErrorPolicy,
    ) -> (TestResultRow, LlmResponse) {
        match self.run_test_once(cfg, tc).await {
            Ok(res) => res,
            Err(e) => Self::error_row_and_output(cfg, tc, e, error_policy),
        }
    }

    fn error_row_and_output(
        cfg: &EvalConfig,
        tc: &TestCase,
        e: anyhow::Error,
        error_policy: ErrorPolicy,
    ) -> (TestResultRow, LlmResponse) {
        let msg = if let Some(diag) = try_map_error(&e) {
            diag.to_string()
        } else {
            e.to_string()
        };

        let policy_result = error_policy.apply_to_error(&e);
        let (status, final_msg, applied_policy) = match policy_result {
            ErrorPolicyResult::Blocked { reason } => {
                (TestStatus::Error, reason, ErrorPolicy::Block)
            }
            ErrorPolicyResult::Allowed { warning } => {
                crate::on_error::log_fail_safe(&warning, None);
                (TestStatus::AllowedOnError, warning, ErrorPolicy::Allow)
            }
        };
        let run_error = e
            .downcast_ref::<RunError>()
            .cloned()
            .unwrap_or_else(|| RunError::from_anyhow(&e));
        let run_error_kind = match &run_error.kind {
            RunErrorKind::TraceNotFound => "trace_not_found",
            RunErrorKind::MissingConfig => "missing_config",
            RunErrorKind::ConfigParse => "config_parse",
            RunErrorKind::InvalidArgs => "invalid_args",
            RunErrorKind::ProviderRateLimit => "provider_rate_limit",
            RunErrorKind::ProviderTimeout => "provider_timeout",
            RunErrorKind::ProviderServer => "provider_server",
            RunErrorKind::Network => "network",
            RunErrorKind::JudgeUnavailable => "judge_unavailable",
            RunErrorKind::Other => "other",
        };

        (
            TestResultRow {
                test_id: tc.id.clone(),
                status,
                score: None,
                cached: false,
                message: final_msg,
                details: serde_json::json!({
                    "error": msg,
                    "policy_applied": applied_policy,
                    "run_error_kind": run_error_kind,
                    "run_error_legacy": run_error.legacy_classified,
                    "run_error": {
                        "path": run_error.path,
                        "status": run_error.status,
                        "provider": run_error.provider,
                        "detail": run_error.detail
                    }
                }),
                duration_ms: None,
                fingerprint: None,
                skip_reason: None,
                attempts: None,
                error_policy_applied: Some(applied_policy),
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

    fn record_attempt(attempts: &mut Vec<AttemptRow>, attempt_no: u32, row: &TestResultRow) {
        attempts.push(AttemptRow {
            attempt_no,
            status: row.status,
            message: row.message.clone(),
            duration_ms: row.duration_ms,
            details: row.details.clone(),
        });
    }

    fn should_stop_retries(status: TestStatus) -> bool {
        matches!(
            status,
            TestStatus::Pass | TestStatus::Warn | TestStatus::AllowedOnError | TestStatus::Skipped
        )
    }

    fn no_attempts_row(tc: &TestCase) -> TestResultRow {
        TestResultRow {
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
            error_policy_applied: None,
        }
    }

    fn apply_quarantine_overlay(&self, final_row: &mut TestResultRow, q_reason: Option<&str>) {
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
    }

    fn apply_failure_classification(
        final_row: &mut TestResultRow,
        class: FailureClass,
        attempt_len: usize,
    ) {
        match class {
            FailureClass::Skipped => {
                final_row.status = TestStatus::Skipped;
                // message usually set by run_test_once
            }
            FailureClass::Flaky => {
                final_row.status = TestStatus::Flaky;
                final_row.message = "flake detected (rerun passed)".into();
                final_row.details["flake"] = serde_json::json!({ "attempts": attempt_len });
            }
            FailureClass::Unstable => {
                final_row.status = TestStatus::Unstable;
                final_row.message = "unstable outcomes detected".into();
                final_row.details["unstable"] = serde_json::json!({ "attempts": attempt_len });
            }
            FailureClass::Error => final_row.status = TestStatus::Error,
            FailureClass::DeterministicFail => {
                // Ensures if last attempt was fail, we keep fail status
                final_row.status = TestStatus::Fail;
            }
            FailureClass::DeterministicPass => {
                // Preserve explicit fail-open semantics instead of collapsing into plain pass.
                if final_row.status != TestStatus::AllowedOnError {
                    final_row.status = TestStatus::Pass;
                }
            }
        }
    }

    fn empty_output_for_model(&self, cfg: &EvalConfig) -> LlmResponse {
        LlmResponse {
            text: "".into(),
            provider: self.client.provider_name().to_string(),
            model: cfg.model.clone(),
            cached: false,
            meta: serde_json::json!({}),
        }
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
        let t = cfg.settings.timeout_seconds.unwrap_or(30);
        let fut = self
            .client
            .complete(&tc.input.prompt, tc.input.context.as_deref());
        let resp = timeout(Duration::from_secs(t), fut).await??;
        Ok(resp)
    }

    fn clone_for_task(&self) -> Runner {
        Runner {
            store: self.store.clone(),
            cache: self.cache.clone(),
            client: self.client.clone(),
            metrics: self.metrics.clone(),
            policy: self.policy.clone(),
            _network_guard: None,
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
        tc: &TestCase,
        metric_name: &str,
        suite_defaults: Option<&crate::model::ThresholdingSettings>,
    ) -> (String, Option<f64>) {
        let mut mode = "absolute".to_string();
        let mut max_drop = None;

        if let Some(s) = suite_defaults {
            if let Some(m) = &s.mode {
                mode = m.clone();
            }
            max_drop = s.max_drop;
        }

        if let Some(t) = tc.expected.thresholding_for_metric(metric_name) {
            if t.max_drop.is_some() {
                max_drop = t.max_drop;
            }
        }

        (mode, max_drop)
    }

    // Embeddings logic
    async fn enrich_semantic(
        &self,
        _cfg: &EvalConfig,
        tc: &TestCase,
        resp: &mut LlmResponse,
    ) -> anyhow::Result<()> {
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

    async fn enrich_judge(
        &self,
        cfg: &EvalConfig,
        tc: &TestCase,
        resp: &mut LlmResponse,
    ) -> anyhow::Result<()> {
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

        // E7.2: Derive stable per-test seed from suite seed + test ID
        let test_seed = cfg.settings.seed.map(|s: u64| {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            s.hash(&mut hasher);
            tc.id.hash(&mut hasher);
            hasher.finish()
        });

        judge
            .evaluate(
                &tc.id,
                rubric_id,
                &tc.input,
                &resp.text,
                rubric_version,
                &mut resp.meta,
                test_seed,
            )
            .await?;

        Ok(())
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
}
