use assay_core::cache::vcr::VcrCache;
use assay_core::engine::runner::{RunPolicy, Runner};
use assay_core::metrics_api::{Metric, MetricResult};
use assay_core::model::{
    EvalConfig, Expected, LlmResponse, Settings, TestCase, TestInput, TestStatus,
};
use assay_core::on_error::ErrorPolicy;
use assay_core::providers::llm::fake::FakeClient;
use assay_core::providers::llm::LlmClient;
use assay_core::quarantine::QuarantineMode;
use assay_core::storage::Store;
use async_trait::async_trait;
use serial_test::serial;
use std::sync::{Arc, Mutex};
use tracing::instrument::WithSubscriber;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
struct MockWriter {
    buf: Arc<Mutex<Vec<u8>>>,
}

impl std::io::Write for MockWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf.lock().expect("writer lock").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for MockWriter {
    type Writer = MockWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn setup_span_capture() -> (MockWriter, tracing::Dispatch) {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = MockWriter { buf: buf.clone() };
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer.clone())
        .json()
        .with_span_events(FmtSpan::CLOSE)
        .with_env_filter(EnvFilter::new("info"))
        .finish();

    (writer, tracing::Dispatch::new(subscriber))
}

fn find_named_spans(
    json_lines: &str,
    target_name: &str,
) -> Vec<serde_json::Map<String, serde_json::Value>> {
    let mut spans = Vec::new();

    for line in json_lines.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(span) = value.get("span").and_then(|span| span.as_object()) else {
            continue;
        };
        if span.get("name").and_then(|v| v.as_str()) == Some(target_name) {
            spans.push(span.clone());
        }
    }

    spans
}

struct PassMetric;

#[async_trait]
impl Metric for PassMetric {
    fn name(&self) -> &'static str {
        "pass_metric"
    }

    async fn evaluate(
        &self,
        _tc: &TestCase,
        _expected: &Expected,
        _resp: &LlmResponse,
    ) -> anyhow::Result<MetricResult> {
        Ok(MetricResult::pass(1.0))
    }
}

struct ErrorMetric;

#[async_trait]
impl Metric for ErrorMetric {
    fn name(&self) -> &'static str {
        "error_metric"
    }

    async fn evaluate(
        &self,
        _tc: &TestCase,
        _expected: &Expected,
        _resp: &LlmResponse,
    ) -> anyhow::Result<MetricResult> {
        Err(anyhow::anyhow!("metric exploded"))
    }
}

fn runner_for_span_tests(client: Arc<dyn LlmClient>, metrics: Vec<Arc<dyn Metric>>) -> Runner {
    let store = Store::memory().expect("in-memory store");
    store.init_schema().expect("schema init");
    Runner {
        store: store.clone(),
        cache: VcrCache::new(store),
        client,
        metrics,
        policy: RunPolicy {
            rerun_failures: 0,
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

fn single_test_config() -> EvalConfig {
    EvalConfig {
        version: 1,
        suite: "runner-metric-spans".to_string(),
        model: "fake-model".to_string(),
        settings: Settings {
            parallel: Some(1),
            cache: Some(false),
            seed: Some(1234),
            on_error: ErrorPolicy::Block,
            ..Default::default()
        },
        thresholds: Default::default(),
        otel: Default::default(),
        tests: vec![TestCase {
            id: "t1".to_string(),
            input: TestInput {
                prompt: "metric spans prompt".to_string(),
                context: None,
            },
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

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn runner_metric_spans_record_success_fields() -> anyhow::Result<()> {
    let (writer, dispatch) = setup_span_capture();
    let cfg = single_test_config();
    let client = Arc::new(FakeClient::new("fake-model".to_string()).with_response("ok".into()));
    let runner = runner_for_span_tests(client, vec![Arc::new(PassMetric)]);

    let artifacts = runner
        .run_suite(&cfg, None)
        .with_subscriber(dispatch)
        .await?;
    let row = artifacts.results.first().expect("result row");
    assert_eq!(row.status, TestStatus::Pass);

    let output = String::from_utf8(writer.buf.lock().expect("writer lock").clone())?;
    let spans = find_named_spans(&output, "assay.eval.metric");
    assert_eq!(spans.len(), 1, "expected one metric span, got: {output}");

    let span = &spans[0];
    assert_eq!(
        span.get("assay.eval.test_id").and_then(|v| v.as_str()),
        Some("t1")
    );
    assert_eq!(
        span.get("assay.eval.metric.name").and_then(|v| v.as_str()),
        Some("pass_metric")
    );
    assert_eq!(
        span.get("assay.eval.response.cached")
            .and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        span.get("assay.eval.metric.score").and_then(|v| v.as_f64()),
        Some(1.0)
    );
    assert_eq!(
        span.get("assay.eval.metric.passed")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        span.get("assay.eval.metric.unstable")
            .and_then(|v| v.as_bool()),
        Some(false)
    );
    assert!(
        span.get("assay.eval.metric.duration_ms")
            .and_then(|v| v.as_u64())
            .is_some(),
        "expected duration field in span: {output}"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn runner_metric_spans_record_error_fields() -> anyhow::Result<()> {
    let (writer, dispatch) = setup_span_capture();
    let cfg = single_test_config();
    let client = Arc::new(FakeClient::new("fake-model".to_string()).with_response("ok".into()));
    let runner = runner_for_span_tests(client, vec![Arc::new(ErrorMetric)]);

    let artifacts = runner
        .run_suite(&cfg, None)
        .with_subscriber(dispatch)
        .await?;
    let row = artifacts.results.first().expect("result row");
    assert_eq!(row.status, TestStatus::Error);
    assert!(
        row.message.contains("metric exploded"),
        "unexpected metric failure row: {}",
        row.message
    );

    let output = String::from_utf8(writer.buf.lock().expect("writer lock").clone())?;
    let spans = find_named_spans(&output, "assay.eval.metric");
    assert_eq!(spans.len(), 1, "expected one metric span, got: {output}");

    let span = &spans[0];
    assert_eq!(
        span.get("assay.eval.metric.name").and_then(|v| v.as_str()),
        Some("error_metric")
    );
    assert_eq!(span.get("error").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        span.get("error.message").and_then(|v| v.as_str()),
        Some("metric exploded")
    );
    assert!(
        span.get("assay.eval.metric.duration_ms")
            .and_then(|v| v.as_u64())
            .is_some(),
        "expected duration field in error span: {output}"
    );
    Ok(())
}
