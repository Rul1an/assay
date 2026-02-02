use crate::config::otel::OtelConfig;
use crate::model::LlmResponse;
use crate::otel::genai::GenAiSpanBuilder;
use crate::otel::redaction::RedactionService;
use crate::providers::llm::LlmClient;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{info_span, Instrument};

pub struct TracingLlmClient {
    inner: Arc<dyn LlmClient>,
    span_builder: GenAiSpanBuilder,
    redaction: RedactionService,
    config: OtelConfig,
}

impl TracingLlmClient {
    pub fn new(inner: Arc<dyn LlmClient>, config: OtelConfig) -> Self {
        let span_builder = GenAiSpanBuilder::new(&config);
        let redaction =
            RedactionService::new(config.capture_mode.clone(), config.redaction.clone());

        Self {
            inner,
            span_builder,
            redaction,
            config,
        }
    }
}

#[async_trait]
impl LlmClient for TracingLlmClient {
    async fn complete(
        &self,
        prompt: &str,
        context: Option<&[String]>,
    ) -> anyhow::Result<LlmResponse> {
        // Compute GenAI attributes
        let (_sys_key, sys_val) = self.span_builder.gen_ai_system();
        let provider = self.inner.provider_name();

        // Conditional Span Creation (Sign-off: gen_ai.prompt only in RedactedInline)
        // Off/BlobRef do NOT declare gen_ai.prompt in info_span! so the key is physically absent in export.
        let span = match self.config.capture_mode {
            crate::config::otel::PromptCaptureMode::Off => {
                info_span!(
                    "gen_ai.client.request",
                    "gen_ai.system" = sys_val,
                    "assay.provider" = provider,
                    "assay.semconv.genai" = self.config.genai_semconv_version.as_str(),
                    "gen_ai.request.model" = tracing::field::Empty,
                    "gen_ai.usage.input_tokens" = tracing::field::Empty,
                    "gen_ai.usage.output_tokens" = tracing::field::Empty,
                    "assay.cached" = tracing::field::Empty,
                    "error" = tracing::field::Empty,
                    "error.message" = tracing::field::Empty
                )
            }
            crate::config::otel::PromptCaptureMode::BlobRef => {
                info_span!(
                    "gen_ai.client.request",
                    "gen_ai.system" = sys_val,
                    "assay.provider" = provider,
                    "assay.semconv.genai" = self.config.genai_semconv_version.as_str(),
                    "gen_ai.request.model" = tracing::field::Empty,
                    "gen_ai.usage.input_tokens" = tracing::field::Empty,
                    "gen_ai.usage.output_tokens" = tracing::field::Empty,
                    "assay.cached" = tracing::field::Empty,
                    "error" = tracing::field::Empty,
                    "error.message" = tracing::field::Empty,
                    "assay.blob.ref" = tracing::field::Empty,
                    "assay.blob.kind" = tracing::field::Empty
                )
            }
            crate::config::otel::PromptCaptureMode::RedactedInline => {
                info_span!(
                    "gen_ai.client.request",
                    "gen_ai.system" = sys_val,
                    "assay.provider" = provider,
                    "assay.semconv.genai" = self.config.genai_semconv_version.as_str(),
                    "gen_ai.request.model" = tracing::field::Empty,
                    "gen_ai.usage.input_tokens" = tracing::field::Empty,
                    "gen_ai.usage.output_tokens" = tracing::field::Empty,
                    "assay.cached" = tracing::field::Empty,
                    "error" = tracing::field::Empty,
                    "error.message" = tracing::field::Empty,
                    "gen_ai.prompt" = tracing::field::Empty
                )
            }
        };

        async move {
            let start = std::time::Instant::now();
            let result = self.inner.complete(prompt, context).await;
            let _duration = start.elapsed();

            let span = tracing::Span::current();

            // capture_requires_sampled_span: gate payload work on "span is recorded".
            // We use !is_disabled() as proxy for is_recording() (tracing 0.1 has no is_recording()).
            // When the subscriber filters out the span (sampling drop), no blob hash / redaction is done.
            if !self.config.capture_requires_sampled_span || !span.is_disabled() {
                if self.redaction.should_capture() {
                    if self.redaction.is_blob_ref() {
                        let blob_ref = self.redaction.blob_ref(prompt);
                        span.record("assay.blob.ref", blob_ref.as_str());
                        span.record("assay.blob.kind", "prompt");
                    } else {
                        let redacted = self.redaction.redact_inline(prompt);
                        span.record("gen_ai.prompt", redacted.as_str());
                    }
                }
            }

            match &result {
                Ok(resp) => {
                    span.record("gen_ai.request.model", resp.model.as_str());
                    span.record("assay.cached", resp.cached);

                    if let Some(usage) = resp.meta.get("usage") {
                        if let Some(i) = usage.get("input_tokens").and_then(|v| v.as_u64()) {
                            span.record("gen_ai.usage.input_tokens", i);
                        }
                        if let Some(o) = usage.get("output_tokens").and_then(|v| v.as_u64()) {
                            span.record("gen_ai.usage.output_tokens", o);
                        }
                    }
                }
                Err(e) => {
                    span.record("error", true);
                    span.record("error.message", e.to_string().as_str());
                }
            }

            result
        }
        .instrument(span)
        .await
    }

    fn provider_name(&self) -> &'static str {
        self.inner.provider_name()
    }

    fn fingerprint(&self) -> Option<String> {
        self.inner.fingerprint()
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;
    use crate::config::otel::{OtelConfig, PromptCaptureMode};
    use crate::providers::llm::fake::FakeClient;
    use serial_test::serial;

    #[tokio::test]
    async fn test_tracing_redaction_inline() {
        let mut cfg = OtelConfig::default();
        cfg.capture_mode = PromptCaptureMode::RedactedInline;
        cfg.capture_acknowledged = true;
        cfg.capture_requires_sampled_span = false;
        cfg.redaction.policies = vec!["sk-".to_string()];

        let inner = Arc::new(FakeClient::new("gpt-4".to_string()));
        let client = TracingLlmClient::new(inner, cfg);

        let res = client.complete("my secret password=123", None).await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_tracing_blob_ref() {
        let mut cfg = OtelConfig::default();
        cfg.capture_mode = PromptCaptureMode::BlobRef;
        cfg.capture_acknowledged = true;
        cfg.capture_requires_sampled_span = false;

        let inner = Arc::new(FakeClient::new("gpt-4".to_string()));
        let client = TracingLlmClient::new(inner, cfg);

        let res = client.complete("my secret", None).await;
        assert!(res.is_ok());
    }

    #[test]
    #[serial]
    fn test_guardrails_validation() {
        let mut cfg = OtelConfig::default();
        cfg.capture_mode = PromptCaptureMode::RedactedInline;
        cfg.capture_acknowledged = true;
        cfg.exporter.allowlist = None;

        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        }
        let res = cfg.validate();
        assert!(res.is_err());

        cfg.exporter.allowlist = Some(vec!["example.com".to_string()]);

        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "http://example.com");
        }
        let res = cfg.validate();
        assert!(res.is_err());

        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "https://example.com");
        }
        let res = cfg.validate();
        assert!(res.is_ok());

        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        }
    }
}
