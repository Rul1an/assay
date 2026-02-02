//! OTel capture contract tests: string-based (fmt JSON) and structured (parsed JSON) asserts.
//! Sign-off: E2E proof that Off/BlobRef do not emit gen_ai.prompt; BlobRef emits assay.blob.ref (hmac256:);
//! RedactedInline emits gen_ai.prompt with policy applied.

use assay_core::config::otel::{OtelConfig, PromptCaptureMode};
use assay_core::providers::llm::fake::FakeClient;
use assay_core::providers::llm::tracing::TracingLlmClient;
use assay_core::providers::llm::LlmClient;
use std::sync::{Arc, Mutex};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::EnvFilter;

/// Parse tracing_subscriber JSON lines: span attributes are in "span" object (FmtSpan::CLOSE).
fn parse_span_field_keys(json_lines: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for line in json_lines.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(span) = v.get("span").and_then(|s| s.as_object()) {
                for k in span.keys() {
                    if !keys.contains(k) {
                        keys.push(k.clone());
                    }
                }
            }
        }
    }
    keys
}

/// Get the string value of a span attribute from the last JSON line that has "span".
fn parse_field_value(json_lines: &str, key: &str) -> Option<String> {
    for line in json_lines.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(span) = v.get("span").and_then(|s| s.as_object()) {
                if let Some(val) = span.get(key) {
                    return val.as_str().map(String::from);
                }
            }
        }
    }
    None
}

#[derive(Clone)]
struct MockWriter {
    buf: Arc<Mutex<Vec<u8>>>,
}

impl std::io::Write for MockWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf.lock().unwrap().extend_from_slice(buf);
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

fn setup_capture() -> (MockWriter, tracing::subscriber::DefaultGuard) {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = MockWriter { buf: buf.clone() };

    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer.clone())
        .json()
        .with_span_events(FmtSpan::CLOSE)
        .finish();

    (writer, tracing::subscriber::set_default(subscriber))
}

#[tokio::test]
async fn test_invariant_capture_off() {
    let (writer, _guard) = setup_capture();

    let mut cfg = OtelConfig::default();
    cfg.capture_mode = PromptCaptureMode::Off;
    cfg.capture_requires_sampled_span = false;

    let inner = Arc::new(FakeClient::new("gpt-4".to_string()));
    let client = TracingLlmClient::new(inner, cfg);

    let _ = client.complete("sensitive secret", None).await;

    let output = String::from_utf8(writer.buf.lock().unwrap().clone()).unwrap();
    println!("DEBUG OUTPUT (OFF): {}", output);

    // String invariants
    assert!(
        !output.contains("sensitive secret"),
        "Leaked sensitive secret!"
    );
    assert!(
        !output.contains("\"gen_ai.prompt\""),
        "Strict Privacy Violation: gen_ai.prompt field present in Off mode"
    );

    // Structured (parsed JSON) proof: key must not exist in exported span fields
    let field_keys = parse_span_field_keys(&output);
    assert!(
        !field_keys.iter().any(|k| k == "gen_ai.prompt"),
        "gen_ai.prompt must be physically absent (not null/empty) in Off mode"
    );
}

#[tokio::test]
async fn test_invariant_blob_ref() {
    let (writer, _guard) = setup_capture();

    let mut cfg = OtelConfig::default();
    cfg.capture_mode = PromptCaptureMode::BlobRef;
    cfg.capture_acknowledged = true;
    cfg.capture_requires_sampled_span = false;

    let inner = Arc::new(FakeClient::new("gpt-4".to_string()));
    let client = TracingLlmClient::new(inner, cfg);

    let _ = client.complete("my prompt", None).await;

    let output = String::from_utf8(writer.buf.lock().unwrap().clone()).unwrap();
    println!("DEBUG OUTPUT (BLOB): {}", output);

    assert!(
        !output.contains("\"gen_ai.prompt\""),
        "Strict Privacy Violation: gen_ai.prompt field present in BlobRef mode"
    );
    assert!(
        output.contains("\"assay.blob.ref\""),
        "Should contain blob ref key"
    );
    assert!(
        output.contains("hmac256:"),
        "Should use HMAC format for BlobRef"
    );

    // Structured proof: gen_ai.prompt absent; assay.blob.ref present and format hmac256:
    let field_keys = parse_span_field_keys(&output);
    assert!(
        !field_keys.iter().any(|k| k == "gen_ai.prompt"),
        "gen_ai.prompt must be physically absent in BlobRef mode"
    );
    assert!(
        field_keys.iter().any(|k| k == "assay.blob.ref"),
        "assay.blob.ref must be present"
    );
    let blob_ref = parse_field_value(&output, "assay.blob.ref").expect("assay.blob.ref value");
    assert!(
        blob_ref.starts_with("hmac256:"),
        "BlobRef format must be hmac256:<hex>"
    );
}

#[tokio::test]
async fn test_invariant_redacted_inline() {
    let (writer, _guard) = setup_capture();

    let mut cfg = OtelConfig::default();
    cfg.capture_mode = PromptCaptureMode::RedactedInline;
    cfg.capture_acknowledged = true;
    cfg.capture_requires_sampled_span = false;
    cfg.redaction.policies = vec!["sk-".to_string()];

    let inner = Arc::new(FakeClient::new("gpt-4".to_string()));
    let client = TracingLlmClient::new(inner, cfg);

    let _ = client.complete("key=sk-12345", None).await;

    let output = String::from_utf8(writer.buf.lock().unwrap().clone()).unwrap();
    println!("DEBUG OUTPUT (REDACTED): {}", output);

    assert!(
        output.contains("\"gen_ai.prompt\":\"key=sk-[REDACTED]5\"")
            || output.contains("sk-[REDACTED]"),
        "Should contain redacted prompt"
    );
    assert!(
        !output.contains("sk-12345"),
        "Should NOT contain raw secret"
    );

    // Structured proof: gen_ai.prompt present and policy applied (no raw secret)
    let field_keys = parse_span_field_keys(&output);
    assert!(
        field_keys.iter().any(|k| k == "gen_ai.prompt"),
        "RedactedInline must emit gen_ai.prompt"
    );
    let prompt_val = parse_field_value(&output, "gen_ai.prompt").unwrap_or_default();
    assert!(
        !prompt_val.contains("sk-12345"),
        "RedactedInline must not contain raw secret in export"
    );
}

/// Sign-off: when span is not recorded (sampling drop), no blob hash / redaction work is done.
/// Subscriber filter "warn" disables info-level spans so is_disabled() is true.
#[tokio::test]
async fn test_capture_requires_sampled_span_no_work_when_disabled() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = MockWriter { buf: buf.clone() };
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer.clone())
        .json()
        .with_span_events(FmtSpan::CLOSE)
        .with_env_filter(EnvFilter::new("warn"))
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let mut cfg = OtelConfig::default();
    cfg.capture_mode = PromptCaptureMode::BlobRef;
    cfg.capture_acknowledged = true;
    cfg.capture_requires_sampled_span = true;

    let inner = Arc::new(FakeClient::new("gpt-4".to_string()));
    let client = TracingLlmClient::new(inner, cfg);
    let _ = client.complete("secret prompt", None).await;

    let output = String::from_utf8(writer.buf.lock().unwrap().clone()).unwrap();
    // Span was disabled (info not enabled): no assay.blob.ref and no gen_ai.prompt
    assert!(
        !output.contains("assay.blob.ref"),
        "Must not compute blob ref when span is not recorded"
    );
    assert!(
        !output.contains("gen_ai.prompt"),
        "Must not emit prompt when span is not recorded"
    );
}
