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

/// CLI Argument enum for Exit Code Version
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum ExitCodeVersion {
    V1,
    #[default]
    V2,
}

// (Removed invalid impl From<ExitCodeVersion> for assay_core::reason::ExitCodeVersion)

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

// Aliases matching previous inline module
pub const OK: i32 = EXIT_SUCCESS;
pub const TEST_FAILED: i32 = EXIT_TEST_FAILURE;
pub const CONFIG_ERROR: i32 = EXIT_CONFIG_ERROR;

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
    /// Get the corresponding exit code for this reason, respecting version
    pub fn exit_code_for(&self, version: ExitCodeVersion) -> i32 {
        match version {
            ExitCodeVersion::V1 => self.exit_code_v1(),
            ExitCodeVersion::V2 => self.exit_code_v2(),
        }
    }

    /// Default exit code (V2)
    pub fn exit_code(&self) -> i32 {
        self.exit_code_v2()
    }

    fn exit_code_v2(&self) -> i32 {
        match self {
            ReasonCode::Success => EXIT_SUCCESS,

            // V2: Config/User errors -> 2
            ReasonCode::ECfgParse
            | ReasonCode::ETraceNotFound
            | ReasonCode::EMissingConfig
            | ReasonCode::EBaselineInvalid
            | ReasonCode::EPolicyParse
            | ReasonCode::EInvalidArgs => EXIT_CONFIG_ERROR,

            // V2: Infra errors -> 3
            ReasonCode::EJudgeUnavailable
            | ReasonCode::ERateLimit
            | ReasonCode::EProvider5xx
            | ReasonCode::ETimeout
            | ReasonCode::ENetworkError => EXIT_INFRA_ERROR,

            // V2: Test failures -> 1
            ReasonCode::ETestFailed
            | ReasonCode::EPolicyViolation
            | ReasonCode::ESequenceViolation
            | ReasonCode::EArgSchema => EXIT_TEST_FAILURE,
        }
    }

    fn exit_code_v1(&self) -> i32 {
        // Legacy mapping (V1)
        match self {
            ReasonCode::Success => EXIT_SUCCESS,

            // In V1, we often conflated errors.
            // E.g., Trace Not Found might have been 3 (Infra) or 1 (General).
            // User spec says: "Trace Not Found is now exit code 2 ... not 3".
            // So V1 TraceNotFound = 3.
            ReasonCode::ETraceNotFound => EXIT_INFRA_ERROR,

            // Most others standard?
            // Assuming config errors were 2, but let's stick to V2 where possible unless specific compat needed.
            _ => self.exit_code_v2(),
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl RunOutcome {
    /// Create a success outcome
    pub fn success() -> Self {
        Self {
            exit_code: EXIT_SUCCESS,
            reason_code: String::new(),
            message: None,
            next_step: None,
            warnings: Vec::new(),
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
            warnings: Vec::new(),
        }
    }

    /// Create an outcome for test failures
    pub fn test_failure(failed_count: usize) -> Self {
        Self {
            exit_code: EXIT_TEST_FAILURE,
            reason_code: ReasonCode::ETestFailed.as_str().to_string(),
            message: Some(format!("{} test(s) failed", failed_count)),
            next_step: Some("Run: assay explain <test-id> for details".to_string()),
            warnings: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_code_constants() {
        assert_eq!(EXIT_SUCCESS, 0);
        assert_eq!(EXIT_TEST_FAILURE, 1);
        assert_eq!(EXIT_CONFIG_ERROR, 2);
        assert_eq!(EXIT_INFRA_ERROR, 3);
        assert_eq!(EXIT_WOULD_BLOCK, 4);
    }

    #[test]
    fn test_reason_code_exit_mapping() {
        // Success maps to 0
        assert_eq!(ReasonCode::Success.exit_code(), EXIT_SUCCESS);

        // Config errors map to 2
        assert_eq!(ReasonCode::ECfgParse.exit_code(), EXIT_CONFIG_ERROR);
        assert_eq!(ReasonCode::ETraceNotFound.exit_code(), EXIT_CONFIG_ERROR);
        assert_eq!(ReasonCode::EMissingConfig.exit_code(), EXIT_CONFIG_ERROR);
        assert_eq!(ReasonCode::EBaselineInvalid.exit_code(), EXIT_CONFIG_ERROR);
        assert_eq!(ReasonCode::EPolicyParse.exit_code(), EXIT_CONFIG_ERROR);
        assert_eq!(ReasonCode::EInvalidArgs.exit_code(), EXIT_CONFIG_ERROR);

        // Infra errors map to 3
        assert_eq!(ReasonCode::EJudgeUnavailable.exit_code(), EXIT_INFRA_ERROR);
        assert_eq!(ReasonCode::ERateLimit.exit_code(), EXIT_INFRA_ERROR);
        assert_eq!(ReasonCode::EProvider5xx.exit_code(), EXIT_INFRA_ERROR);
        assert_eq!(ReasonCode::ETimeout.exit_code(), EXIT_INFRA_ERROR);
        assert_eq!(ReasonCode::ENetworkError.exit_code(), EXIT_INFRA_ERROR);

        // Test failures map to 1
        assert_eq!(ReasonCode::ETestFailed.exit_code(), EXIT_TEST_FAILURE);
        assert_eq!(ReasonCode::EPolicyViolation.exit_code(), EXIT_TEST_FAILURE);
        assert_eq!(
            ReasonCode::ESequenceViolation.exit_code(),
            EXIT_TEST_FAILURE
        );
        assert_eq!(ReasonCode::EArgSchema.exit_code(), EXIT_TEST_FAILURE);
    }

    #[test]
    fn test_reason_code_as_str() {
        assert_eq!(ReasonCode::Success.as_str(), "");
        assert_eq!(ReasonCode::ECfgParse.as_str(), "E_CFG_PARSE");
        assert_eq!(ReasonCode::ETraceNotFound.as_str(), "E_TRACE_NOT_FOUND");
        assert_eq!(ReasonCode::EMissingConfig.as_str(), "E_MISSING_CONFIG");
        assert_eq!(ReasonCode::EBaselineInvalid.as_str(), "E_BASELINE_INVALID");
        assert_eq!(ReasonCode::EPolicyParse.as_str(), "E_POLICY_PARSE");
        assert_eq!(ReasonCode::EInvalidArgs.as_str(), "E_INVALID_ARGS");
        assert_eq!(
            ReasonCode::EJudgeUnavailable.as_str(),
            "E_JUDGE_UNAVAILABLE"
        );
        assert_eq!(ReasonCode::ERateLimit.as_str(), "E_RATE_LIMIT");
        assert_eq!(ReasonCode::EProvider5xx.as_str(), "E_PROVIDER_5XX");
        assert_eq!(ReasonCode::ETimeout.as_str(), "E_TIMEOUT");
        assert_eq!(ReasonCode::ENetworkError.as_str(), "E_NETWORK_ERROR");
        assert_eq!(ReasonCode::ETestFailed.as_str(), "E_TEST_FAILED");
        assert_eq!(ReasonCode::EPolicyViolation.as_str(), "E_POLICY_VIOLATION");
        assert_eq!(
            ReasonCode::ESequenceViolation.as_str(),
            "E_SEQUENCE_VIOLATION"
        );
        assert_eq!(ReasonCode::EArgSchema.as_str(), "E_ARG_SCHEMA");
    }

    #[test]
    fn test_reason_code_next_step() {
        // Success returns empty string
        assert!(ReasonCode::Success.next_step(None).is_empty());

        // Config errors provide actionable next steps
        assert!(ReasonCode::ECfgParse
            .next_step(Some("test.yaml"))
            .contains("test.yaml"));
        assert!(ReasonCode::ETraceNotFound
            .next_step(Some("traces/ci.jsonl"))
            .contains("traces/ci.jsonl"));
        assert!(ReasonCode::EMissingConfig
            .next_step(None)
            .contains("assay init"));
        assert!(ReasonCode::EBaselineInvalid
            .next_step(None)
            .contains("baseline"));
        assert!(ReasonCode::EPolicyParse
            .next_step(None)
            .contains("policy validate"));
        assert!(ReasonCode::EInvalidArgs.next_step(None).contains("--help"));

        // Infra errors provide recovery guidance
        assert!(ReasonCode::EJudgeUnavailable
            .next_step(None)
            .contains("provider"));
        assert!(ReasonCode::ERateLimit
            .next_step(None)
            .contains("rate limit"));
        assert!(ReasonCode::EProvider5xx.next_step(None).contains("retry"));
        assert!(ReasonCode::ETimeout.next_step(None).contains("timeout"));
        assert!(ReasonCode::ENetworkError
            .next_step(None)
            .contains("network"));

        // Test failures point to explain command
        assert!(ReasonCode::ETestFailed
            .next_step(None)
            .contains("assay explain"));
        assert!(ReasonCode::EPolicyViolation
            .next_step(None)
            .contains("explain"));
        assert!(ReasonCode::ESequenceViolation
            .next_step(None)
            .contains("explain"));
        assert!(ReasonCode::EArgSchema.next_step(None).contains("explain"));
    }

    #[test]
    fn test_reason_code_display() {
        assert_eq!(
            format!("{}", ReasonCode::ETraceNotFound),
            "E_TRACE_NOT_FOUND"
        );
        assert_eq!(format!("{}", ReasonCode::Success), "");
    }

    #[test]
    fn test_run_outcome_success() {
        let outcome = RunOutcome::success();
        assert_eq!(outcome.exit_code, EXIT_SUCCESS);
        assert_eq!(outcome.reason_code, "");
        assert!(outcome.message.is_none());
        assert!(outcome.next_step.is_none());
    }

    #[test]
    fn test_run_outcome_from_reason() {
        let outcome = RunOutcome::from_reason(
            ReasonCode::ETraceNotFound,
            Some("File not found: test.jsonl".to_string()),
            Some("test.jsonl"),
        );
        assert_eq!(outcome.exit_code, EXIT_CONFIG_ERROR);
        assert_eq!(outcome.reason_code, "E_TRACE_NOT_FOUND");
        assert!(outcome.message.as_ref().unwrap().contains("test.jsonl"));
        assert!(outcome.next_step.as_ref().unwrap().contains("test.jsonl"));
    }

    #[test]
    fn test_run_outcome_test_failure() {
        let outcome = RunOutcome::test_failure(3);
        assert_eq!(outcome.exit_code, EXIT_TEST_FAILURE);
        assert_eq!(outcome.reason_code, "E_TEST_FAILED");
        assert!(outcome.message.as_ref().unwrap().contains("3 test(s)"));
        assert!(outcome.next_step.as_ref().unwrap().contains("explain"));
    }

    #[test]
    fn test_run_outcome_serialization() {
        let outcome = RunOutcome::test_failure(2);
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("\"exit_code\":1"));
        assert!(json.contains("\"reason_code\":\"E_TEST_FAILED\""));
        assert!(json.contains("2 test(s) failed"));
    }
}
