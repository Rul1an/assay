//! Unified exit codes and reason codes for Assay CLI.
//!
//! Exit codes are **coarse** (0/1/2/3) for CI compatibility.
//! Reason codes provide **fine-grained**, machine-readable semantics.
//!
//! See: SPEC-PR-Gate-Outputs-v1.md for the full contract.

use serde::{Deserialize, Serialize};

// ============================================================================
// Exit Codes (coarse, stable)
// ============================================================================

/// All tests passed
pub const EXIT_SUCCESS: i32 = 0;

/// One or more tests failed
pub const EXIT_TEST_FAILURE: i32 = 1;

/// Configuration or user error (config parse, trace not found, etc.)
pub const EXIT_CONFIG_ERROR: i32 = 2;

/// Infrastructure or judge unavailable (rate limit, provider 5xx, timeout)
pub const EXIT_INFRA_ERROR: i32 = 3;

/// Would block (dry-run mode) - sandbox-specific
pub const EXIT_WOULD_BLOCK: i32 = 4;

// Legacy aliases for backward compatibility
pub const SUCCESS: i32 = EXIT_SUCCESS;
pub const COMMAND_FAILED: i32 = EXIT_TEST_FAILURE;
pub const INTERNAL_ERROR: i32 = EXIT_CONFIG_ERROR;
pub const POLICY_UNENFORCEABLE: i32 = EXIT_CONFIG_ERROR;
pub const VIOLATION_AUDIT: i32 = EXIT_INFRA_ERROR;
pub const WOULD_BLOCK: i32 = EXIT_WOULD_BLOCK;

// ============================================================================
// Reason Codes (fine-grained, machine-readable)
// ============================================================================

/// Reason code registry per SPEC-PR-Gate-Outputs-v1
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReasonCode {
    // Success (exit 0)
    /// All tests passed
    Success,

    // Config / User Error (exit 2)
    /// Config file parse error (YAML/JSON)
    ECfgParse,
    /// Trace file or path not found
    ETraceNotFound,
    /// Required config file missing
    EMissingConfig,
    /// Baseline file invalid or missing
    EBaselineInvalid,
    /// Policy file parse error
    EPolicyParse,
    /// Invalid command-line arguments
    EInvalidArgs,

    // Infra / Judge Unavailable (exit 3)
    /// Judge service unavailable or returned error
    EJudgeUnavailable,
    /// Judge/provider rate limit hit
    ERateLimit,
    /// Judge/provider returned 5xx
    EProvider5xx,
    /// Judge or dependency timed out
    ETimeout,
    /// Network error (connection refused, DNS failure)
    ENetworkError,

    // Test Failure (exit 1)
    /// One or more tests failed
    ETestFailed,
    /// Policy violation detected
    EPolicyViolation,
    /// Sequence assertion failed
    ESequenceViolation,
    /// Argument schema validation failed
    EArgSchema,
}

impl ReasonCode {
    /// Get the corresponding exit code for this reason
    pub fn exit_code(&self) -> i32 {
        match self {
            ReasonCode::Success => EXIT_SUCCESS,

            // Config errors -> exit 2
            ReasonCode::ECfgParse
            | ReasonCode::ETraceNotFound
            | ReasonCode::EMissingConfig
            | ReasonCode::EBaselineInvalid
            | ReasonCode::EPolicyParse
            | ReasonCode::EInvalidArgs => EXIT_CONFIG_ERROR,

            // Infra errors -> exit 3
            ReasonCode::EJudgeUnavailable
            | ReasonCode::ERateLimit
            | ReasonCode::EProvider5xx
            | ReasonCode::ETimeout
            | ReasonCode::ENetworkError => EXIT_INFRA_ERROR,

            // Test failures -> exit 1
            ReasonCode::ETestFailed
            | ReasonCode::EPolicyViolation
            | ReasonCode::ESequenceViolation
            | ReasonCode::EArgSchema => EXIT_TEST_FAILURE,
        }
    }

    /// Get the string representation for summary.json
    pub fn as_str(&self) -> &'static str {
        match self {
            ReasonCode::Success => "",
            ReasonCode::ECfgParse => "E_CFG_PARSE",
            ReasonCode::ETraceNotFound => "E_TRACE_NOT_FOUND",
            ReasonCode::EMissingConfig => "E_MISSING_CONFIG",
            ReasonCode::EBaselineInvalid => "E_BASELINE_INVALID",
            ReasonCode::EPolicyParse => "E_POLICY_PARSE",
            ReasonCode::EInvalidArgs => "E_INVALID_ARGS",
            ReasonCode::EJudgeUnavailable => "E_JUDGE_UNAVAILABLE",
            ReasonCode::ERateLimit => "E_RATE_LIMIT",
            ReasonCode::EProvider5xx => "E_PROVIDER_5XX",
            ReasonCode::ETimeout => "E_TIMEOUT",
            ReasonCode::ENetworkError => "E_NETWORK_ERROR",
            ReasonCode::ETestFailed => "E_TEST_FAILED",
            ReasonCode::EPolicyViolation => "E_POLICY_VIOLATION",
            ReasonCode::ESequenceViolation => "E_SEQUENCE_VIOLATION",
            ReasonCode::EArgSchema => "E_ARG_SCHEMA",
        }
    }

    /// Suggested next step for this error
    pub fn next_step(&self, context: Option<&str>) -> String {
        match self {
            ReasonCode::Success => String::new(),
            ReasonCode::ECfgParse => {
                format!(
                    "Run: assay doctor --config {}",
                    context.unwrap_or("<config.yaml>")
                )
            }
            ReasonCode::ETraceNotFound => {
                format!(
                    "Check trace file exists: {}",
                    context.unwrap_or("<trace.jsonl>")
                )
            }
            ReasonCode::EMissingConfig => "Run: assay init to create a config file".to_string(),
            ReasonCode::EBaselineInvalid => {
                "Run: assay baseline record to create a new baseline".to_string()
            }
            ReasonCode::EPolicyParse => {
                format!(
                    "Run: assay policy validate {}",
                    context.unwrap_or("<policy.yaml>")
                )
            }
            ReasonCode::EInvalidArgs => "Run: assay --help for usage".to_string(),
            ReasonCode::EJudgeUnavailable => {
                "Check judge/LLM provider status and API key".to_string()
            }
            ReasonCode::ERateLimit => {
                "Retry after rate limit window or reduce concurrency".to_string()
            }
            ReasonCode::EProvider5xx => {
                "Provider error; retry or check provider status page".to_string()
            }
            ReasonCode::ETimeout => "Increase timeout or check network connectivity".to_string(),
            ReasonCode::ENetworkError => {
                "Check network connectivity and firewall rules".to_string()
            }
            ReasonCode::ETestFailed => "Run: assay explain <test-id> for details".to_string(),
            ReasonCode::EPolicyViolation => {
                "Run: assay explain <test-id> or review policy rules".to_string()
            }
            ReasonCode::ESequenceViolation => {
                "Run: assay explain <test-id> to see sequence mismatch".to_string()
            }
            ReasonCode::EArgSchema => {
                "Run: assay explain <test-id> to see schema violation".to_string()
            }
        }
    }
}

impl std::fmt::Display for ReasonCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// Run Outcome (combines exit code, reason, message, next step)
// ============================================================================

/// Structured outcome for a run, suitable for summary.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunOutcome {
    pub exit_code: i32,
    pub reason_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_step: Option<String>,
}

impl RunOutcome {
    /// Create a success outcome
    pub fn success() -> Self {
        Self {
            exit_code: EXIT_SUCCESS,
            reason_code: String::new(),
            message: None,
            next_step: None,
        }
    }

    /// Create an outcome from a reason code
    pub fn from_reason(reason: ReasonCode, message: Option<String>, context: Option<&str>) -> Self {
        let next_step = if reason != ReasonCode::Success {
            Some(reason.next_step(context))
        } else {
            None
        };
        Self {
            exit_code: reason.exit_code(),
            reason_code: reason.as_str().to_string(),
            message,
            next_step,
        }
    }

    /// Create an outcome for test failures
    pub fn test_failure(failed_count: usize) -> Self {
        Self {
            exit_code: EXIT_TEST_FAILURE,
            reason_code: ReasonCode::ETestFailed.as_str().to_string(),
            message: Some(format!("{} test(s) failed", failed_count)),
            next_step: Some("Run: assay explain <test-id> for details".to_string()),
        }
    }
}
