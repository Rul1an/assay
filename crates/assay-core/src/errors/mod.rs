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
    pub path: Option<String>,
    pub status: Option<u16>,
    pub provider: Option<String>,
    pub detail: Option<String>,
    /// True when kind was inferred from free-form message parsing.
    pub legacy_classified: bool,
}

impl RunError {
    pub fn new(kind: RunErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            path: None,
            status: None,
            provider: None,
            detail: None,
            legacy_classified: false,
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn trace_not_found(path: impl Into<String>, detail: impl Into<String>) -> Self {
        let path = path.into();
        let detail = detail.into();
        Self::new(
            RunErrorKind::TraceNotFound,
            format!("Trace not found: {}", path),
        )
        .with_path(path)
        .with_detail(detail)
    }

    pub fn missing_config(path: impl Into<String>, detail: impl Into<String>) -> Self {
        let path = path.into();
        let detail = detail.into();
        Self::new(
            RunErrorKind::MissingConfig,
            format!("Config file not found: {}", path),
        )
        .with_path(path)
        .with_detail(detail)
    }

    pub fn config_parse(path: Option<String>, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        let mut err = Self::new(RunErrorKind::ConfigParse, detail.clone()).with_detail(detail);
        if let Some(path) = path {
            err = err.with_path(path);
        }
        err
    }

    pub fn invalid_args(detail: impl Into<String>) -> Self {
        let detail = detail.into();
        Self::new(RunErrorKind::InvalidArgs, detail.clone()).with_detail(detail)
    }

    pub fn provider_rate_limit(
        status: u16,
        provider: Option<String>,
        detail: impl Into<String>,
    ) -> Self {
        let detail = detail.into();
        let mut err = Self::new(RunErrorKind::ProviderRateLimit, detail.clone())
            .with_status(status)
            .with_detail(detail);
        if let Some(provider) = provider {
            err = err.with_provider(provider);
        }
        err
    }

    pub fn provider_timeout(provider: Option<String>, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        let mut err = Self::new(RunErrorKind::ProviderTimeout, detail.clone()).with_detail(detail);
        if let Some(provider) = provider {
            err = err.with_provider(provider);
        }
        err
    }

    pub fn provider_server(
        status: Option<u16>,
        provider: Option<String>,
        detail: impl Into<String>,
    ) -> Self {
        let detail = detail.into();
        let mut err = Self::new(RunErrorKind::ProviderServer, detail.clone()).with_detail(detail);
        if let Some(status) = status {
            err = err.with_status(status);
        }
        if let Some(provider) = provider {
            err = err.with_provider(provider);
        }
        err
    }

    pub fn network(provider: Option<String>, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        let mut err = Self::new(RunErrorKind::Network, detail.clone()).with_detail(detail);
        if let Some(provider) = provider {
            err = err.with_provider(provider);
        }
        err
    }

    pub fn judge_unavailable(provider: Option<String>, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        let mut err = Self::new(RunErrorKind::JudgeUnavailable, detail.clone()).with_detail(detail);
        if let Some(provider) = provider {
            err = err.with_provider(provider);
        }
        err
    }

    pub fn other(detail: impl Into<String>) -> Self {
        let detail = detail.into();
        Self::new(RunErrorKind::Other, detail.clone()).with_detail(detail)
    }

    pub fn classify_message(message: impl Into<String>) -> Self {
        Self::legacy_classify_message(message)
    }

    pub fn legacy_classify_message(message: impl Into<String>) -> Self {
        let message = message.into();
        let msg = message.to_lowercase();
        let has_not_found_signal = msg.contains("no such file")
            || msg.contains("not found")
            || msg.contains("cannot find")
            || msg.contains("can't find")
            || msg.contains("could not find")
            || msg.contains("os error 2");

        let kind = if msg.contains("trace not found")
            || msg.contains("tracenotfound")
            || msg.contains("failed to load trace")
            || (msg.contains("failed to ingest trace") && has_not_found_signal)
            || (msg.contains("trace") && has_not_found_signal)
        {
            RunErrorKind::TraceNotFound
        } else if msg.contains("failed to ingest trace") {
            RunErrorKind::ConfigParse
        } else if msg.contains("no config found")
            || msg.contains("config missing")
            || msg.contains("config file not found")
            || (msg.contains("failed to read config") && has_not_found_signal)
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

        let mut run_error = Self::new(kind, message);
        run_error.legacy_classified = true;
        run_error
    }

    pub fn from_anyhow(err: &anyhow::Error) -> Self {
        Self::legacy_classify_message(err.to_string())
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
                "Baseline incompatible with current run",
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
            RunError::classify_message(
                "ConfigError: failed to read config eval.yaml: No such file or directory (os error 2)"
            )
            .kind,
            RunErrorKind::MissingConfig
        );
        assert_eq!(
            RunError::classify_message("config error: unknown field `foo`").kind,
            RunErrorKind::ConfigParse
        );
        assert_eq!(
            RunError::classify_message("Failed to ingest trace: invalid JSON on line 1").kind,
            RunErrorKind::ConfigParse
        );
    }

    #[test]
    fn classify_message_does_not_misclassify_ingest_errors_as_not_found() {
        assert_ne!(
            RunError::classify_message("Failed to ingest trace: unsupported schema_version").kind,
            RunErrorKind::TraceNotFound
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

    #[test]
    fn typed_constructors_capture_stable_fields() {
        let trace = RunError::trace_not_found("traces/missing.jsonl", "os error 2");
        assert_eq!(trace.kind, RunErrorKind::TraceNotFound);
        assert_eq!(trace.path.as_deref(), Some("traces/missing.jsonl"));
        assert_eq!(trace.detail.as_deref(), Some("os error 2"));
        assert!(!trace.legacy_classified);

        let provider = RunError::provider_server(
            Some(503),
            Some("openai".to_string()),
            "provider unavailable",
        );
        assert_eq!(provider.kind, RunErrorKind::ProviderServer);
        assert_eq!(provider.status, Some(503));
        assert_eq!(provider.provider.as_deref(), Some("openai"));
        assert!(!provider.legacy_classified);
    }

    #[test]
    fn legacy_classification_is_explicitly_marked() {
        let legacy = RunError::classify_message("provider returned 429");
        assert!(legacy.legacy_classified);
    }
}
