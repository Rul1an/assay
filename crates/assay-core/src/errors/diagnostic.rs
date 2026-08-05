use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: String,
    pub source: String,
    pub message: String,
    pub context: serde_json::Value,
    pub fix_steps: Vec<String>,
}

/// Severity icons, written as escapes so the source stays ASCII and the exact
/// codepoints (including the variation selector on the warning sign) are visible.
const ICON_ERROR: &str = "\u{274c} ";
const ICON_WARN: &str = "\u{26a0}\u{fe0f} ";

impl Diagnostic {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity: "error".into(), // Default to error
            source: "unknown".into(),
            message: message.into(),
            context: serde_json::json!({}),
            fix_steps: vec![],
        }
    }

    pub fn with_severity(mut self, severity: impl Into<String>) -> Self {
        self.severity = severity.into();
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = context;
        self
    }

    pub fn with_fix_step(mut self, step: impl Into<String>) -> Self {
        self.fix_steps.push(step.into());
        self
    }

    /// Decorated rendering for an interactive terminal.
    pub fn format_terminal(&self) -> String {
        let icon = if self.severity == "warn" {
            ICON_WARN
        } else {
            ICON_ERROR
        };
        self.render(icon)
    }

    /// Undecorated rendering for pipes, CI logs and files.
    ///
    /// This is not a synonym for `format_terminal`. Callers reach for it when the
    /// sink is not a terminal, where an emoji is noise a log grep has to work
    /// around rather than information.
    pub fn format_plain(&self) -> String {
        self.render("")
    }

    fn render(&self, prefix: &str) -> String {
        let mut s = format!("{}[{}] {}\n", prefix, self.code, self.message);
        s.push_str(&format!("  source: {}\n", self.source));

        // Simple pretty print for context if not empty object
        if !self.context.is_null() && self.context.as_object().is_some_and(|o| !o.is_empty()) {
            if let Ok(json) = serde_json::to_string_pretty(&self.context) {
                // Indent context
                for line in json.lines() {
                    s.push_str(&format!("  {}\n", line));
                }
            }
        }

        if !self.fix_steps.is_empty() {
            s.push_str("\nFix:\n");
            for (i, step) in self.fix_steps.iter().enumerate() {
                s.push_str(&format!("  {}. {}\n", i + 1, step));
            }
        }
        s
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_terminal())
    }
}

impl std::error::Error for Diagnostic {}

// Common error codes
pub mod codes {
    // Errors (Exit 2)
    pub const E_CFG_PARSE: &str = "E_CFG_PARSE";
    pub const E_CFG_SCHEMA: &str = "E_CFG_SCHEMA";
    pub const E_PATH_NOT_FOUND: &str = "E_PATH_NOT_FOUND";
    pub const E_TRACE_MISS: &str = "E_TRACE_MISS";
    pub const E_TRACE_INVALID: &str = "E_TRACE_INVALID";
    pub const E_BASE_MISMATCH: &str = "E_BASE_MISMATCH";
    pub const E_REPLAY_STRICT_MISSING: &str = "E_REPLAY_STRICT_MISSING";
    pub const E_EMB_DIMS: &str = "E_EMB_DIMS";
    pub const E_POLICY_VIOLATION: &str = "E_POLICY_VIOLATION";

    // Warnings (Exit 0)
    /// A test that asserts nothing: no `expected:` block and no `assertions:`, so it
    /// passes for any response. An `expected:` block written out as empty is rejected
    /// at parse time as `E_CFG_PARSE` instead.
    pub const W_CFG_VACUOUS_EXPECTED: &str = "W_CFG_VACUOUS_EXPECTED";
    pub const W_BASE_FINGERPRINT: &str = "W_BASE_FINGERPRINT";
    pub const W_CACHE_CONFUSION: &str = "W_CACHE_CONFUSION";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Diagnostic {
        Diagnostic::new(codes::E_CFG_PARSE, "mapping values are not allowed here")
            .with_source("config")
            .with_context(serde_json::json!({ "path": "assay.yaml" }))
            .with_fix_step("Run: assay doctor --config assay.yaml")
    }

    #[test]
    fn plain_carries_no_terminal_decoration() {
        let plain = sample().format_plain();
        assert!(
            plain.is_ascii(),
            "plain output must stay ASCII for CI logs: {plain:?}"
        );
        assert!(plain.starts_with("[E_CFG_PARSE]"));

        let warn_plain = sample().with_severity("warn").format_plain();
        assert!(warn_plain.is_ascii(), "warnings must be plain too");
    }

    #[test]
    fn terminal_carries_the_severity_icon() {
        let error = sample().format_terminal();
        assert!(error.starts_with(ICON_ERROR));

        let warn = sample().with_severity("warn").format_terminal();
        assert!(warn.starts_with(ICON_WARN));
    }

    #[test]
    fn the_prefix_is_the_only_difference() {
        let d = sample();
        assert_eq!(
            d.format_terminal().strip_prefix(ICON_ERROR),
            Some(d.format_plain().as_str())
        );
    }

    #[test]
    fn body_carries_code_source_context_and_fix() {
        let plain = sample().format_plain();
        assert!(plain.contains("E_CFG_PARSE"));
        assert!(plain.contains("source: config"));
        assert!(plain.contains("assay.yaml"));
        assert!(plain.contains("1. Run: assay doctor --config assay.yaml"));
    }
}
