pub mod diagnostic;
pub mod similarity;

pub use diagnostic::Diagnostic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunErrorKind {
    TraceNotFound,
    MissingConfig,
    ConfigParse,
    InvalidArgs,
    ProviderRateLimit,
    ProviderTimeout,
    ProviderServer,
    Network,
    JudgeUnavailable,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunError {
    pub kind: RunErrorKind,
    pub message: String,
}

impl RunError {
    pub fn classify_message(message: impl Into<String>) -> Self {
        let message = message.into();
        let msg = message.to_lowercase();

        let kind = if msg.contains("trace not found")
            || msg.contains("tracenotfound")
            || msg.contains("failed to load trace")
            || msg.contains("failed to ingest trace")
            || (msg.contains("trace") && msg.contains("no such file"))
        {
            RunErrorKind::TraceNotFound
        } else if msg.contains("no config found")
            || msg.contains("config missing")
            || msg.contains("config file not found")
        {
            RunErrorKind::MissingConfig
        } else if msg.contains("cannot use --")
            || msg.contains("invalid argument")
            || msg.contains("invalid args")
        {
            RunErrorKind::InvalidArgs
        } else if msg.contains("config error")
            || msg.contains("configerror")
            || msg.contains("failed to parse yaml")
            || msg.contains("unknown field")
        {
            RunErrorKind::ConfigParse
        } else if msg.contains("rate limit") || msg.contains("429") {
            RunErrorKind::ProviderRateLimit
        } else if msg.contains("timeout") {
            RunErrorKind::ProviderTimeout
        } else if msg.contains("500")
            || msg.contains("502")
            || msg.contains("503")
            || msg.contains("504")
            || msg.contains("provider error")
        {
            RunErrorKind::ProviderServer
        } else if msg.contains("network") || msg.contains("connection") || msg.contains("dns") {
            RunErrorKind::Network
        } else if msg.contains("judge unavailable")
            || msg.contains("judge error")
            || msg.contains("judge failed")
        {
            RunErrorKind::JudgeUnavailable
        } else {
            RunErrorKind::Other
        };

        Self { kind, message }
    }

    pub fn from_anyhow(err: &anyhow::Error) -> Self {
        Self::classify_message(err.to_string())
    }
}

/// Helper to map common anyhow errors to structured Diagnostics
pub fn try_map_error(err: &anyhow::Error) -> Option<Diagnostic> {
    // 1) First, try to downcast if it's already a Diagnostic
    if let Some(diag) = err.downcast_ref::<Diagnostic>() {
        return Some(diag.clone());
    }

    let msg = err.to_string();

    // Mapping embedded dimension mismatch
    // "embedding dims mismatch" is a string we expect from runners/providers
    // Ideally providers return typed errors, but string matching is pragmatic for v0.3.x
    if msg.contains("embedding dims mismatch") || msg.contains("dimension mismatch") {
        // We could try to parse "expected A, got B" if the message format is stable
        // For now, actionable generic advice
        return Some(
            Diagnostic::new(
                diagnostic::codes::E_EMB_DIMS,
                "Embedding dimensions mismatch",
            )
            .with_context(serde_json::json!({ "raw_error": msg }))
            .with_fix_step("Run: assay trace precompute-embeddings --trace <file> ...")
            .with_fix_step(
                "Ensure the same embedding model is used for baseline and candidate runs",
            ),
        );
    }

    // Baseline mismatch
    if msg.contains("Baseline mismatch") || (msg.contains("baseline") && msg.contains("schema")) {
        return Some(
            Diagnostic::new(
                diagnostic::codes::E_BASE_MISMATCH,
                "Baseline incompatbile with current run",
            )
            .with_context(serde_json::json!({ "raw_error": msg }))
            .with_fix_step("Regenerate baseline on main branch: assay ci --export-baseline ...")
            .with_fix_step("Check that your config suite name matches the baseline suite"),
        );
    }

    None
}

use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub struct ConfigError(pub String);

impl Display for ConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ConfigError: {}", self.0)
    }
}
impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::{RunError, RunErrorKind};

    #[test]
    fn classify_message_maps_config_errors() {
        assert_eq!(
            RunError::classify_message("trace not found: traces/missing.jsonl").kind,
            RunErrorKind::TraceNotFound
        );
        assert_eq!(
            RunError::classify_message("config file not found: eval.yaml").kind,
            RunErrorKind::MissingConfig
        );
        assert_eq!(
            RunError::classify_message("config error: unknown field `foo`").kind,
            RunErrorKind::ConfigParse
        );
    }

    #[test]
    fn classify_message_maps_infra_errors() {
        assert_eq!(
            RunError::classify_message("provider returned 429").kind,
            RunErrorKind::ProviderRateLimit
        );
        assert_eq!(
            RunError::classify_message("request timeout while calling provider").kind,
            RunErrorKind::ProviderTimeout
        );
        assert_eq!(
            RunError::classify_message("provider error: 503").kind,
            RunErrorKind::ProviderServer
        );
        assert_eq!(
            RunError::classify_message("network dns resolution failed").kind,
            RunErrorKind::Network
        );
    }
}
