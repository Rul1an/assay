use crate::config::otel::OtelConfig;
use crate::otel::semconv::{GenAiSemConv, V1_28_0};

pub struct GenAiSpanBuilder {
    semconv: Box<dyn GenAiSemConv + Send + Sync>,
}

impl GenAiSpanBuilder {
    pub fn new(cfg: &OtelConfig) -> Self {
        let semconv: Box<dyn GenAiSemConv + Send + Sync> = match cfg.genai_semconv_version.as_str()
        {
            "1.28.0" => Box::new(V1_28_0::new(cfg.semconv_stability.clone())),
            _ => {
                // Fallback to latest known or error.
                // Plan said Pinned, so default to 1.28.0 for now.
                Box::new(V1_28_0::new(cfg.semconv_stability.clone()))
            }
        };
        Self { semconv }
    }

    pub fn gen_ai_system(&self) -> (&'static str, &'static str) {
        (self.semconv.system(), "assay")
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

    // Helper to get privacy-aware prompt key
    // Returns (key, is_experimental)
    pub fn prompt_key(&self) -> &'static str {
        self.semconv.prompt_content()
    }
}
