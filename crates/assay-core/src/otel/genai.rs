//! Compatibility facade for the 6.3.1 `otel::genai` path.
//!
//! [`GenAiSpanBuilder::new`](crate::otel::genai::GenAiSpanBuilder::new) keeps
//! the historical silent fallback (unknown versions still construct) so the
//! public constructor stays non-breaking. Production emit and ingest use
//! [`require_known_semconv_version`](crate::otel::pin::require_known_semconv_version)
//! and fail closed. Keys still come from [`pin`].

use crate::config::otel::OtelConfig;
use crate::otel::pin;
use crate::otel::semconv::{GenAiSemConv, V1_28_0};

/// Span-attribute helper kept at its 6.3.1 path.
///
/// Unknown `genai_semconv_version` values still construct a builder. That is
/// the old documented fallback, not the production pin path.
pub struct GenAiSpanBuilder {
    semconv: Box<dyn GenAiSemConv + Send + Sync>,
}

impl GenAiSpanBuilder {
    pub fn new(cfg: &OtelConfig) -> Self {
        // Unknown versions still construct (historical silent fallback).
        // OtelConfig::validate / require_known_semconv_version fail closed.
        Self {
            semconv: Box::new(V1_28_0::new(cfg.semconv_stability.clone())),
        }
    }

    pub fn gen_ai_system(&self) -> (&'static str, &'static str) {
        (self.semconv.system(), pin::PROVIDER_ASSAY)
    }

    pub fn request_model(&self) -> &'static str {
        self.semconv.request_model()
    }

    pub fn usage_input_tokens(&self) -> &'static str {
        self.semconv.usage_input_tokens()
    }

    pub fn usage_output_tokens(&self) -> &'static str {
        self.semconv.usage_output_tokens()
    }

    pub fn prompt_key(&self) -> &'static str {
        self.semconv.prompt_content()
    }
}
