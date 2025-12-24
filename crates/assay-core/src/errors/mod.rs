pub mod diagnostic;
pub mod similarity;

pub use diagnostic::Diagnostic;

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
