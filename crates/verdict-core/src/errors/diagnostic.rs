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

    pub fn format_terminal(&self) -> String {
        let icon = if self.severity == "warn" {
            "⚠️ "
        } else {
            "❌"
        };
        let mut s = format!("{} [{}] {}\n", icon, self.code, self.message);
        s.push_str(&format!("  source: {}\n", self.source));

        // Simple pretty print for context if not empty object
        if !self.context.is_null() && self.context.as_object().map_or(false, |o| !o.is_empty()) {
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

    pub fn format_plain(&self) -> String {
        // Strip out any ansi codes if we added them (we didn't yet), just text
        self.format_terminal()
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

    // Warnings (Exit 0)
    pub const W_BASE_FINGERPRINT: &str = "W_BASE_FINGERPRINT";
    pub const W_CACHE_CONFUSION: &str = "W_CACHE_CONFUSION";
}
