//! One GenAI semantic-convention pin for emit and ingest.
//!
//! The dedicated repository has no release. A pin is a commit plus the attribute
//! table that commit defines, not a floating version label.

/// Repository and commit Assay's emit and ingest surfaces share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemconvPin {
    pub repo: &'static str,
    pub commit: &'static str,
    /// Upstream README still lists the schema URL as TODO.
    pub schema_url: Option<&'static str>,
}

/// `repo@commit` form. The ingest lockfile `upstream_sources[type=semconv].commit`
/// is the suffix after `@`.
pub const GENAI_SEMCONV_PIN: &str =
    "open-telemetry/semantic-conventions-genai@434c91dcc34ed038e3048c07720ddfed2c6bddfc";

pub const ATTR_PROVIDER_NAME: &str = "gen_ai.provider.name";
pub const ATTR_OPERATION_NAME: &str = "gen_ai.operation.name";
pub const ATTR_TOOL_NAME: &str = "gen_ai.tool.name";
pub const PROVIDER_ASSAY: &str = "assay";

/// Retired GenAI system attribute. Named here so the `otel::semconv`
/// facade cannot invent a second string. Emit and ingest write [`ATTR_PROVIDER_NAME`].
pub const ATTR_SYSTEM: &str = "gen_ai.system";
pub const ATTR_REQUEST_MODEL: &str = "gen_ai.request.model";
pub const ATTR_REQUEST_TEMPERATURE: &str = "gen_ai.request.temperature";
pub const ATTR_REQUEST_TOP_P: &str = "gen_ai.request.top_p";
pub const ATTR_USAGE_INPUT_TOKENS: &str = "gen_ai.usage.input_tokens";
pub const ATTR_USAGE_OUTPUT_TOKENS: &str = "gen_ai.usage.output_tokens";
pub const ATTR_RESPONSE_FINISH_REASONS: &str = "gen_ai.response.finish_reasons";
pub const ATTR_RESPONSE_ID: &str = "gen_ai.response.id";
pub const ATTR_RESPONSE_MODEL: &str = "gen_ai.response.model";
pub const ATTR_PROMPT: &str = "gen_ai.prompt";
pub const ATTR_COMPLETION: &str = "gen_ai.completion";

/// Split [`GENAI_SEMCONV_PIN`] into repo and commit. One string, derived fields.
pub fn genai_semconv() -> SemconvPin {
    let (repo, commit) = GENAI_SEMCONV_PIN
        .split_once('@')
        .expect("GENAI_SEMCONV_PIN is repo@commit");
    SemconvPin {
        repo,
        commit,
        schema_url: None,
    }
}

/// Unknown version fails closed. There is no silent fallback to 1.28.0.
pub fn require_known_semconv_version(version: &str) -> Result<(), String> {
    let pin = genai_semconv();
    if version == GENAI_SEMCONV_PIN || version == pin.commit {
        Ok(())
    } else {
        Err(format!(
            "unknown genai_semconv_version {version:?}; pinned revision is {GENAI_SEMCONV_PIN}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_commit_matches_as_str_suffix() {
        let pin = genai_semconv();
        assert_eq!(GENAI_SEMCONV_PIN, format!("{}@{}", pin.repo, pin.commit));
        assert_eq!(pin.commit.len(), 40);
    }
}
