//! Compatibility facade for the 6.3.1 `otel::semconv` path.
//!
//! Attribute names and the version string come from [`crate::otel::pin`].
//! Production emit and ingest call the pin; this module must not become a
//! second version table.

use crate::config::otel::SemConvStability;
use crate::otel::pin;

/// Trait to name OpenTelemetry GenAI attributes across versions.
///
/// Prefer [`crate::otel::pin`]. This trait remains reachable so 6.3.1 stays
/// non-breaking. `#[deprecated]` is omitted: cargo-semver-checks treats that
/// as a minor bump, and this crate is still on the 6.3.1 patch line.
pub trait GenAiSemConv {
    fn version(&self) -> &'static str;

    fn system(&self) -> &'static str;

    fn request_model(&self) -> &'static str;
    fn request_temperature(&self) -> &'static str;
    fn request_top_p(&self) -> &'static str;

    fn usage_input_tokens(&self) -> &'static str;
    fn usage_output_tokens(&self) -> &'static str;

    fn response_finish_reasons(&self) -> &'static str;
    fn response_id(&self) -> &'static str;
    fn response_model(&self) -> &'static str;

    fn prompt_content(&self) -> &'static str;
    fn completion_content(&self) -> &'static str;
}

/// Historical type name for the current pin's attribute table.
///
/// The name is kept for cargo-semver-checks. [`GenAiSemConv::version`]
/// returns [`GENAI_SEMCONV_PIN`](crate::otel::pin::GENAI_SEMCONV_PIN),
/// not the retired `1.28.0` label.
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
        pin::GENAI_SEMCONV_PIN
    }

    fn system(&self) -> &'static str {
        pin::ATTR_SYSTEM
    }

    fn request_model(&self) -> &'static str {
        pin::ATTR_REQUEST_MODEL
    }
    fn request_temperature(&self) -> &'static str {
        pin::ATTR_REQUEST_TEMPERATURE
    }
    fn request_top_p(&self) -> &'static str {
        pin::ATTR_REQUEST_TOP_P
    }

    fn usage_input_tokens(&self) -> &'static str {
        pin::ATTR_USAGE_INPUT_TOKENS
    }
    fn usage_output_tokens(&self) -> &'static str {
        pin::ATTR_USAGE_OUTPUT_TOKENS
    }

    fn response_finish_reasons(&self) -> &'static str {
        pin::ATTR_RESPONSE_FINISH_REASONS
    }
    fn response_id(&self) -> &'static str {
        pin::ATTR_RESPONSE_ID
    }
    fn response_model(&self) -> &'static str {
        pin::ATTR_RESPONSE_MODEL
    }

    fn prompt_content(&self) -> &'static str {
        pin::ATTR_PROMPT
    }

    fn completion_content(&self) -> &'static str {
        pin::ATTR_COMPLETION
    }
}
