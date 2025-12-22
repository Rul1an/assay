//! Diagnostic system for actionable error messages.
//!
//! Every error in Verdict includes:
//! - A stable error code (E001, E002, etc.)
//! - A human-readable message
//! - Actionable fix steps (1-3 bullets)
//! - Rich context for debugging

use serde::{Deserialize, Serialize};
use std::fmt;

/// Stable error codes for all Verdict diagnostics.
///
/// These codes are stable across versions and can be referenced in documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DiagnosticCode {
    // Trace errors (E001-E019)
    /// No matching trace entry found for test prompt
    E001TraceMiss,
    /// Trace file not found or unreadable
    E002TraceFileNotFound,
    /// Trace schema version mismatch
    E003TraceSchemaInvalid,
    /// Trace entry malformed (invalid JSON)
    E004TraceEntryMalformed,

    // Baseline errors (E020-E039)
    /// Baseline file not found
    E020BaselineNotFound,
    /// Baseline suite name mismatch
    E021BaselineSuiteMismatch,
    /// Baseline schema version mismatch
    E022BaselineSchemaVersionMismatch,
    /// Baseline fingerprint mismatch (config changed)
    E023BaselineFingerprintMismatch,

    // Embedding errors (E040-E059)
    /// Embedding dimensions mismatch between cached and live
    E040EmbeddingDimsMismatch,
    /// Embedding model ID mismatch
    E041EmbeddingModelMismatch,
    /// Embeddings not precomputed for strict replay
    E042EmbeddingsNotPrecomputed,

    // Judge errors (E060-E079)
    /// Judge results not precomputed for strict replay
    E060JudgeNotPrecomputed,
    /// Judge model mismatch
    E061JudgeModelMismatch,
    /// Judge disagreement (voting failed to converge)
    E062JudgeDisagreement,

    // Config errors (E080-E099)
    /// Config file not found
    E080ConfigNotFound,
    /// Config parse error (invalid YAML)
    E081ConfigParseError,
    /// Config validation error (missing required fields)
    E082ConfigValidationError,
    /// Unknown metric type in config
    E083UnknownMetricType,

    // Runtime errors (E100-E119)
    /// Strict replay mode but live call attempted
    E100StrictReplayViolation,
    /// API rate limit exceeded
    E101RateLimitExceeded,
    /// API authentication failed
    E102AuthenticationFailed,
    /// Timeout during LLM call
    E103Timeout,

    // Database errors (E120-E139)
    /// Database migration failed
    E120MigrationFailed,
    /// Database locked
    E121DatabaseLocked,
    /// Database corrupted
    E122DatabaseCorrupted,
}

impl DiagnosticCode {
    /// Returns the string code (e.g., "E001")
    pub fn code(&self) -> &'static str {
        match self {
            Self::E001TraceMiss => "E001",
            Self::E002TraceFileNotFound => "E002",
            Self::E003TraceSchemaInvalid => "E003",
            Self::E004TraceEntryMalformed => "E004",
            Self::E020BaselineNotFound => "E020",
            Self::E021BaselineSuiteMismatch => "E021",
            Self::E022BaselineSchemaVersionMismatch => "E022",
            Self::E023BaselineFingerprintMismatch => "E023",
            Self::E040EmbeddingDimsMismatch => "E040",
            Self::E041EmbeddingModelMismatch => "E041",
            Self::E042EmbeddingsNotPrecomputed => "E042",
            Self::E060JudgeNotPrecomputed => "E060",
            Self::E061JudgeModelMismatch => "E061",
            Self::E062JudgeDisagreement => "E062",
            Self::E080ConfigNotFound => "E080",
            Self::E081ConfigParseError => "E081",
            Self::E082ConfigValidationError => "E082",
            Self::E083UnknownMetricType => "E083",
            Self::E100StrictReplayViolation => "E100",
            Self::E101RateLimitExceeded => "E101",
            Self::E102AuthenticationFailed => "E102",
            Self::E103Timeout => "E103",
            Self::E120MigrationFailed => "E120",
            Self::E121DatabaseLocked => "E121",
            Self::E122DatabaseCorrupted => "E122",
        }
    }

    /// Returns a short description of the error category
    pub fn category(&self) -> &'static str {
        match self {
            Self::E001TraceMiss
            | Self::E002TraceFileNotFound
            | Self::E003TraceSchemaInvalid
            | Self::E004TraceEntryMalformed => "Trace",

            Self::E020BaselineNotFound
            | Self::E021BaselineSuiteMismatch
            | Self::E022BaselineSchemaVersionMismatch
            | Self::E023BaselineFingerprintMismatch => "Baseline",

            Self::E040EmbeddingDimsMismatch
            | Self::E041EmbeddingModelMismatch
            | Self::E042EmbeddingsNotPrecomputed => "Embedding",

            Self::E060JudgeNotPrecomputed
            | Self::E061JudgeModelMismatch
            | Self::E062JudgeDisagreement => "Judge",

            Self::E080ConfigNotFound
            | Self::E081ConfigParseError
            | Self::E082ConfigValidationError
            | Self::E083UnknownMetricType => "Config",

            Self::E100StrictReplayViolation
            | Self::E101RateLimitExceeded
            | Self::E102AuthenticationFailed
            | Self::E103Timeout => "Runtime",

            Self::E120MigrationFailed
            | Self::E121DatabaseLocked
            | Self::E122DatabaseCorrupted => "Database",
        }
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code())
    }
}

/// Rich context for a diagnostic, varies by error type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DiagnosticContext {
    /// Context for trace miss errors
    TraceMiss {
        test_id: String,
        expected_prompt: String,
        closest_match: Option<ClosestMatch>,
    },

    /// Context for baseline mismatch errors
    BaselineMismatch {
        expected_suite: String,
        found_suite: String,
        expected_schema_version: String,
        found_schema_version: String,
    },

    /// Context for embedding dimension mismatch
    EmbeddingMismatch {
        test_id: String,
        expected_dims: usize,
        found_dims: usize,
        expected_model: String,
        found_model: String,
    },

    /// Context for strict replay violation
    StrictReplayViolation {
        test_id: String,
        missing_data: Vec<String>, // e.g., ["embeddings", "judge"]
    },

    /// Context for config errors
    ConfigError {
        file_path: String,
        line: Option<usize>,
        column: Option<usize>,
        snippet: Option<String>,
    },

    /// Generic context with key-value pairs
    Generic {
        details: std::collections::HashMap<String, String>,
    },
}

/// Information about a closest matching trace entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosestMatch {
    /// The prompt text of the closest match
    pub prompt: String,
    /// Similarity score (0.0 - 1.0)
    pub similarity: f64,
    /// Character positions where the difference starts
    pub diff_positions: Vec<DiffPosition>,
}

/// A position where two strings differ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffPosition {
    /// Character index where difference starts
    pub start: usize,
    /// Character index where difference ends
    pub end: usize,
    /// The expected substring
    pub expected: String,
    /// The found substring
    pub found: String,
}

/// A complete diagnostic with all information needed for user-facing errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Stable error code
    pub code: DiagnosticCode,
    /// Short, one-line message
    pub message: String,
    /// Actionable fix steps (1-3 bullets)
    pub fix_steps: Vec<String>,
    /// Rich context for debugging
    pub context: DiagnosticContext,
}

impl Diagnostic {
    /// Create a new diagnostic.
    pub fn new(
        code: DiagnosticCode,
        message: impl Into<String>,
        context: DiagnosticContext,
    ) -> Self {
        let fix_steps = Self::default_fix_steps(code, &context);
        Self {
            code,
            message: message.into(),
            fix_steps,
            context,
        }
    }

    /// Create a diagnostic with custom fix steps.
    pub fn with_fix_steps(
        code: DiagnosticCode,
        message: impl Into<String>,
        context: DiagnosticContext,
        fix_steps: Vec<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            fix_steps,
            context,
        }
    }

    /// Generate default fix steps based on error code and context.
    fn default_fix_steps(code: DiagnosticCode, context: &DiagnosticContext) -> Vec<String> {
        match code {
            DiagnosticCode::E001TraceMiss => vec![
                "Update your prompt template to match the trace entry".to_string(),
                "Or re-record: verdict trace ingest --input <new-traces.jsonl>".to_string(),
                "Or verify coverage: verdict trace verify --config <config.yaml>".to_string(),
            ],

            DiagnosticCode::E002TraceFileNotFound => vec![
                "Check the --trace-file path is correct".to_string(),
                "Generate traces: verdict trace ingest --input <raw-traces/>".to_string(),
            ],

            DiagnosticCode::E003TraceSchemaInvalid => vec![
                "Re-ingest traces with current schema: verdict trace ingest --input <traces/>".to_string(),
                "Check trace file format matches schema v1".to_string(),
            ],

            DiagnosticCode::E020BaselineNotFound => vec![
                "Create a baseline: verdict baseline save --config <config.yaml>".to_string(),
                "Or run without baseline for initial setup".to_string(),
            ],

            DiagnosticCode::E021BaselineSuiteMismatch => {
                if let DiagnosticContext::BaselineMismatch { expected_suite, found_suite, .. } = context {
                    vec![
                        format!("Update config suite name to '{}' to match baseline", found_suite),
                        format!("Or regenerate baseline for suite '{}'", expected_suite),
                    ]
                } else {
                    vec!["Regenerate baseline: verdict baseline save".to_string()]
                }
            }

            DiagnosticCode::E022BaselineSchemaVersionMismatch => vec![
                "Regenerate baseline with current Verdict version".to_string(),
                "Run: verdict baseline save --config <config.yaml>".to_string(),
            ],

            DiagnosticCode::E040EmbeddingDimsMismatch => {
                if let DiagnosticContext::EmbeddingMismatch { expected_model, found_model, .. } = context {
                    vec![
                        format!("Re-precompute embeddings with model '{}'", expected_model),
                        format!("Run: verdict trace precompute-embeddings --model {}", expected_model),
                        format!("Or update config to use model '{}'", found_model),
                    ]
                } else {
                    vec![
                        "Re-precompute embeddings: verdict trace precompute-embeddings".to_string(),
                    ]
                }
            }

            DiagnosticCode::E042EmbeddingsNotPrecomputed => vec![
                "Precompute embeddings: verdict trace precompute-embeddings --trace <trace.jsonl>".to_string(),
                "Or disable strict replay: remove --replay-strict flag".to_string(),
            ],

            DiagnosticCode::E060JudgeNotPrecomputed => vec![
                "Precompute judge results: verdict trace precompute-judge --trace <trace.jsonl>".to_string(),
                "Or disable strict replay: remove --replay-strict flag".to_string(),
            ],

            DiagnosticCode::E062JudgeDisagreement => vec![
                "Increase voting samples: set judge.samples = 5 in config".to_string(),
                "Review the test case for ambiguity".to_string(),
                "Consider quarantining: verdict quarantine add --test-id <id>".to_string(),
            ],

            DiagnosticCode::E080ConfigNotFound => vec![
                "Check the --config path is correct".to_string(),
                "Create a config: verdict init".to_string(),
            ],

            DiagnosticCode::E081ConfigParseError => vec![
                "Check YAML syntax (indentation, quotes)".to_string(),
                "Validate: verdict validate --config <config.yaml>".to_string(),
            ],

            DiagnosticCode::E100StrictReplayViolation => {
                if let DiagnosticContext::StrictReplayViolation { missing_data, .. } = context {
                    let mut steps = vec![];
                    if missing_data.contains(&"embeddings".to_string()) {
                        steps.push("Precompute embeddings: verdict trace precompute-embeddings".to_string());
                    }
                    if missing_data.contains(&"judge".to_string()) {
                        steps.push("Precompute judge: verdict trace precompute-judge".to_string());
                    }
                    steps.push("Or disable strict mode: remove --replay-strict".to_string());
                    steps
                } else {
                    vec!["Precompute required data or disable strict replay".to_string()]
                }
            }

            DiagnosticCode::E101RateLimitExceeded => vec![
                "Wait and retry, or reduce parallelism in config".to_string(),
                "Use trace replay to avoid live API calls".to_string(),
            ],

            DiagnosticCode::E102AuthenticationFailed => vec![
                "Check OPENAI_API_KEY or ANTHROPIC_API_KEY is set".to_string(),
                "Verify the API key is valid and has sufficient permissions".to_string(),
            ],

            DiagnosticCode::E120MigrationFailed => vec![
                "Backup and delete .eval/eval.db, then re-run".to_string(),
                "Or manually run migrations: verdict db migrate".to_string(),
            ],

            _ => vec!["See documentation for details".to_string()],
        }
    }

    /// Format the diagnostic for terminal output with colors.
    pub fn format_terminal(&self) -> String {
        let mut output = String::new();

        // Header with error code
        output.push_str(&format!(
            "\x1b[1;31mError [{}]\x1b[0m {}\n",
            self.code.code(),
            self.message
        ));

        // Context-specific details
        output.push_str(&self.format_context());

        // Fix steps
        output.push_str("\n\x1b[1;33mFix:\x1b[0m\n");
        for (i, step) in self.fix_steps.iter().enumerate() {
            output.push_str(&format!("  {}. {}\n", i + 1, step));
        }

        output
    }

    /// Format the diagnostic for plain text (logs, CI).
    pub fn format_plain(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!("Error [{}] {}\n", self.code.code(), self.message));
        output.push_str(&self.format_context_plain());
        output.push_str("\nFix:\n");
        for (i, step) in self.fix_steps.iter().enumerate() {
            output.push_str(&format!("  {}. {}\n", i + 1, step));
        }

        output
    }

    fn format_context(&self) -> String {
        match &self.context {
            DiagnosticContext::TraceMiss {
                test_id,
                expected_prompt,
                closest_match,
            } => {
                let mut s = format!("\n  \x1b[1mTest:\x1b[0m {}\n", test_id);
                s.push_str(&format!(
                    "  \x1b[1mExpected:\x1b[0m \"{}\"\n",
                    truncate_str(expected_prompt, 60)
                ));

                if let Some(cm) = closest_match {
                    s.push_str(&format!(
                        "  \x1b[1mClosest:\x1b[0m  \"{}\" \x1b[2m(similarity: {:.2})\x1b[0m\n",
                        truncate_str(&cm.prompt, 60),
                        cm.similarity
                    ));

                    // Show diff positions
                    for diff in &cm.diff_positions {
                        s.push_str(&format!(
                            "            \x1b[31m{}\x1b[0m → \x1b[32m{}\x1b[0m\n",
                            diff.expected, diff.found
                        ));
                    }
                }
                s
            }

            DiagnosticContext::EmbeddingMismatch {
                test_id,
                expected_dims,
                found_dims,
                expected_model,
                found_model,
            } => {
                format!(
                    "\n  \x1b[1mTest:\x1b[0m {}\n  \x1b[1mExpected dims:\x1b[0m {} (model: {})\n  \x1b[1mFound dims:\x1b[0m {} (model: {})\n",
                    test_id, expected_dims, expected_model, found_dims, found_model
                )
            }

            DiagnosticContext::BaselineMismatch {
                expected_suite,
                found_suite,
                expected_schema_version,
                found_schema_version,
            } => {
                format!(
                    "\n  \x1b[1mExpected suite:\x1b[0m {} (schema {})\n  \x1b[1mFound suite:\x1b[0m {} (schema {})\n",
                    expected_suite, expected_schema_version, found_suite, found_schema_version
                )
            }

            DiagnosticContext::StrictReplayViolation {
                test_id,
                missing_data,
            } => {
                format!(
                    "\n  \x1b[1mTest:\x1b[0m {}\n  \x1b[1mMissing:\x1b[0m {}\n",
                    test_id,
                    missing_data.join(", ")
                )
            }

            DiagnosticContext::ConfigError {
                file_path,
                line,
                column,
                snippet,
            } => {
                let mut s = format!("\n  \x1b[1mFile:\x1b[0m {}", file_path);
                if let Some(l) = line {
                    s.push_str(&format!(":{}", l));
                    if let Some(c) = column {
                        s.push_str(&format!(":{}", c));
                    }
                }
                s.push('\n');
                if let Some(snip) = snippet {
                    s.push_str(&format!("  \x1b[2m{}\x1b[0m\n", snip));
                }
                s
            }

            DiagnosticContext::Generic { details } => {
                let mut s = String::new();
                for (k, v) in details {
                    s.push_str(&format!("\n  \x1b[1m{}:\x1b[0m {}", k, v));
                }
                s.push('\n');
                s
            }
        }
    }

    fn format_context_plain(&self) -> String {
        // Same as format_context but without ANSI codes
        match &self.context {
            DiagnosticContext::TraceMiss {
                test_id,
                expected_prompt,
                closest_match,
            } => {
                let mut s = format!("\n  Test: {}\n", test_id);
                s.push_str(&format!(
                    "  Expected: \"{}\"\n",
                    truncate_str(expected_prompt, 60)
                ));

                if let Some(cm) = closest_match {
                    s.push_str(&format!(
                        "  Closest:  \"{}\" (similarity: {:.2})\n",
                        truncate_str(&cm.prompt, 60),
                        cm.similarity
                    ));

                    for diff in &cm.diff_positions {
                        s.push_str(&format!(
                            "            '{}' → '{}'\n",
                            diff.expected, diff.found
                        ));
                    }
                }
                s
            }

            DiagnosticContext::EmbeddingMismatch {
                test_id,
                expected_dims,
                found_dims,
                expected_model,
                found_model,
            } => {
                format!(
                    "\n  Test: {}\n  Expected dims: {} (model: {})\n  Found dims: {} (model: {})\n",
                    test_id, expected_dims, expected_model, found_dims, found_model
                )
            }

            DiagnosticContext::BaselineMismatch {
                expected_suite,
                found_suite,
                expected_schema_version,
                found_schema_version,
            } => {
                format!(
                    "\n  Expected suite: {} (schema {})\n  Found suite: {} (schema {})\n",
                    expected_suite, expected_schema_version, found_suite, found_schema_version
                )
            }

            DiagnosticContext::StrictReplayViolation {
                test_id,
                missing_data,
            } => {
                format!(
                    "\n  Test: {}\n  Missing: {}\n",
                    test_id,
                    missing_data.join(", ")
                )
            }

            DiagnosticContext::ConfigError {
                file_path,
                line,
                column,
                snippet,
            } => {
                let mut s = format!("\n  File: {}", file_path);
                if let Some(l) = line {
                    s.push_str(&format!(":{}", l));
                    if let Some(c) = column {
                        s.push_str(&format!(":{}", c));
                    }
                }
                s.push('\n');
                if let Some(snip) = snippet {
                    s.push_str(&format!("  {}\n", snip));
                }
                s
            }

            DiagnosticContext::Generic { details } => {
                let mut s = String::new();
                for (k, v) in details {
                    s.push_str(&format!("\n  {}: {}", k, v));
                }
                s.push('\n');
                s
            }
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_plain())
    }
}

impl std::error::Error for Diagnostic {}

/// Truncate a string with ellipsis if too long.
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_code_display() {
        assert_eq!(DiagnosticCode::E001TraceMiss.code(), "E001");
        assert_eq!(DiagnosticCode::E040EmbeddingDimsMismatch.code(), "E040");
    }

    #[test]
    fn test_diagnostic_category() {
        assert_eq!(DiagnosticCode::E001TraceMiss.category(), "Trace");
        assert_eq!(DiagnosticCode::E040EmbeddingDimsMismatch.category(), "Embedding");
        assert_eq!(DiagnosticCode::E080ConfigNotFound.category(), "Config");
    }

    #[test]
    fn test_trace_miss_diagnostic() {
        let diag = Diagnostic::new(
            DiagnosticCode::E001TraceMiss,
            "Trace miss for test 't1'",
            DiagnosticContext::TraceMiss {
                test_id: "t1".to_string(),
                expected_prompt: "What is the capital of France?".to_string(),
                closest_match: Some(ClosestMatch {
                    prompt: "What is the capitol of France?".to_string(),
                    similarity: 0.96,
                    diff_positions: vec![DiffPosition {
                        start: 16,
                        end: 23,
                        expected: "capital".to_string(),
                        found: "capitol".to_string(),
                    }],
                }),
            },
        );

        assert_eq!(diag.code, DiagnosticCode::E001TraceMiss);
        assert!(!diag.fix_steps.is_empty());
        
        let output = diag.format_plain();
        assert!(output.contains("E001"));
        assert!(output.contains("capital"));
        assert!(output.contains("capitol"));
    }

    #[test]
    fn test_embedding_mismatch_diagnostic() {
        let diag = Diagnostic::new(
            DiagnosticCode::E040EmbeddingDimsMismatch,
            "Embedding dimensions mismatch",
            DiagnosticContext::EmbeddingMismatch {
                test_id: "t2".to_string(),
                expected_dims: 1536,
                found_dims: 3072,
                expected_model: "text-embedding-3-small".to_string(),
                found_model: "text-embedding-3-large".to_string(),
            },
        );

        let output = diag.format_plain();
        assert!(output.contains("1536"));
        assert!(output.contains("3072"));
        assert!(output.contains("text-embedding-3-small"));
    }

    #[test]
    fn test_diagnostic_serialization() {
        let diag = Diagnostic::new(
            DiagnosticCode::E001TraceMiss,
            "Test",
            DiagnosticContext::Generic {
                details: [("key".to_string(), "value".to_string())]
                    .into_iter()
                    .collect(),
            },
        );

        let json = serde_json::to_string(&diag).unwrap();
        assert!(json.contains("E001TraceMiss"));
    }
}
