use crate::config::otel::SemConvStability;

/// Trait to valid OpenTelemetry GenAI Semantic Conventions across versions.
pub trait GenAiSemConv {
    fn version(&self) -> &'static str;

    // System Attributes
    fn system(&self) -> &'static str;

    // Request Attributes
    fn request_model(&self) -> &'static str;
    fn request_temperature(&self) -> &'static str;
    fn request_top_p(&self) -> &'static str;

    // Usage Attributes
    fn usage_input_tokens(&self) -> &'static str;
    fn usage_output_tokens(&self) -> &'static str;

    // Response Attributes
    fn response_finish_reasons(&self) -> &'static str;
    fn response_id(&self) -> &'static str;
    fn response_model(&self) -> &'static str; // If different from request

    // Payload Attributes (Privacy sensitive)
    fn prompt_content(&self) -> &'static str;
    fn completion_content(&self) -> &'static str;
}

pub struct V1_28_0 {
    #[allow(dead_code)]
    stability: SemConvStability,
}

impl V1_28_0 {
    pub fn new(stability: SemConvStability) -> Self {
        Self { stability }
    }
}

impl GenAiSemConv for V1_28_0 {
    fn version(&self) -> &'static str {
        "1.28.0"
    }

    fn system(&self) -> &'static str {
        "gen_ai.system"
    }

    fn request_model(&self) -> &'static str {
        "gen_ai.request.model"
    }
    fn request_temperature(&self) -> &'static str {
        "gen_ai.request.temperature"
    }
    fn request_top_p(&self) -> &'static str {
        "gen_ai.request.top_p"
    }

    fn usage_input_tokens(&self) -> &'static str {
        "gen_ai.usage.input_tokens"
    }
    fn usage_output_tokens(&self) -> &'static str {
        "gen_ai.usage.output_tokens"
    }

    fn response_finish_reasons(&self) -> &'static str {
        "gen_ai.response.finish_reasons"
    }
    fn response_id(&self) -> &'static str {
        "gen_ai.response.id"
    }
    fn response_model(&self) -> &'static str {
        "gen_ai.response.model"
    }

    fn prompt_content(&self) -> &'static str {
        // If strict stability and this is experimental?
        // In 1.28, prompts might be experimental. For now we return standard key.
        "gen_ai.prompt"
    }

    fn completion_content(&self) -> &'static str {
        "gen_ai.completion"
    }
}
