use crate::attempts::{classify_attempts, FailureClass};
use crate::cache::key::cache_key;
use crate::cache::vcr::VcrCache;
use crate::metrics_api::Metric;
use crate::model::{AttemptRow, EvalConfig, LlmResponse, TestCase, TestResultRow, TestStatus};
use crate::providers::llm::LlmClient;
use crate::quarantine::{QuarantineMode, QuarantineService};
use crate::report::RunArtifacts;
use crate::storage::Store;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{timeout, Duration};

#[derive(Debug, Clone)]
pub struct RunPolicy {
    pub rerun_failures: u32,
    pub quarantine_mode: QuarantineMode,
}

impl Default for RunPolicy {
    fn default() -> Self {
        Self {
            rerun_failures: 1,
            quarantine_mode: QuarantineMode::Warn,
        }
    }
}

pub struct Runner {
    pub store: Store,
    pub cache: VcrCache,
    pub client: Arc<dyn LlmClient>,
    pub metrics: Vec<Arc<dyn Metric>>,
    pub policy: RunPolicy,
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
                },
                Err(e) => TestResultRow {
                    test_id: "unknown".into(),
                    status: TestStatus::Error,
                    score: None,
                    cached: false,
                    message: format!("join error: {}", e),
                    details: serde_json::json!({}),
                    duration_ms: None,
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
            let (row, output) = self.run_test_once(cfg, tc).await?;
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
                TestStatus::Fail | TestStatus::Error | TestStatus::Flaky => continue,
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
            FailureClass::Flake => {
                final_row.status = TestStatus::Flaky;
                final_row.message = "flake detected (rerun passed)".into();
                final_row.details["flake"] = serde_json::json!({ "attempts": attempts.len() });
            }
            FailureClass::Error => final_row.status = TestStatus::Error,
            FailureClass::DeterministicFail => {}
        }

        let output = last_output.unwrap_or(LlmResponse {
            text: "".into(),
            provider: self.client.provider_name().to_string(),
            model: cfg.model.clone(),
            cached: false,
            meta: serde_json::json!({}),
        });
        self.store
            .insert_result_embedded(run_id, &final_row, &attempts, &output)?;

        Ok(final_row)
    }

    async fn run_test_once(
        &self,
        cfg: &EvalConfig,
        tc: &TestCase,
    ) -> anyhow::Result<(TestResultRow, LlmResponse)> {
        let fingerprint = format!("v{}|{}", cfg.version, self.client.provider_name());
        let key = cache_key(&cfg.model, &tc.input.prompt, &fingerprint);

        let start = std::time::Instant::now();
        let mut cached = false;

        let mut resp: LlmResponse = if cfg.settings.cache.unwrap_or(true) {
            if let Some(r) = self.cache.get(&key)? {
                cached = true;
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

        let duration_ms = start.elapsed().as_millis() as u64;
        let row = TestResultRow {
            test_id: tc.id.clone(),
            status: final_status,
            score: final_score,
            cached: resp.cached,
            message: if msg.is_empty() { "ok".into() } else { msg },
            details,
            duration_ms: Some(duration_ms),
        };
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
        }
    }
}

#[derive(Clone)]
struct RunnerRef {
    store: Store,
    cache: VcrCache,
    client: Arc<dyn LlmClient>,
    metrics: Vec<Arc<dyn Metric>>,
    policy: RunPolicy,
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
        };
        runner.run_test_with_policy(cfg, tc, run_id).await
    }
}
