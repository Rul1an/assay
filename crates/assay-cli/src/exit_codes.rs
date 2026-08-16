//! Unified exit codes and reason codes for Assay CLI.
//!
//! Exit codes are **coarse** (0/1/2/3) for CI compatibility.
//! Reason codes provide **fine-grained**, machine-readable semantics.
//!
//! See: SPEC-PR-Gate-Outputs-v1.md for the full contract.

use assay_core::errors::ConfigLoadError;
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;

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
    /// Replay bundle missing required dependency for offline replay
    EReplayMissingDependency,
    /// A replay ingest ceiling refused the bundle. Distinct from a parse failure: the bundle may
    /// be well-formed and simply larger than the configured budget, which is an operator decision
    /// rather than a producer defect.
    EReplayLimitExceeded,
    // The boundary this code draws is normative for #2164 and #2165, so it is stated once, in
    // `exit_codes/evidence_integrity_boundary.md`, and transported here and into the
    // SPEC-PR-Gate-Outputs-v1 §5.1 row. `spec_reason_code_registry.rs` asserts the two carry that
    // one file byte for byte once flowed; a second hand-written copy here could contradict it.
    #[doc = include_str!("exit_codes/evidence_integrity_boundary.md")]
    EEvidenceIntegrity,
    // Companion to `EEvidenceIntegrity` for typed format-contract defects. The boundary is
    // stated once in `exit_codes/evidence_contract_boundary.md` and transported here and into
    // the SPEC-PR-Gate-Outputs-v1 §5.1 row.
    #[doc = include_str!("exit_codes/evidence_contract_boundary.md")]
    EEvidenceContract,
    /// Evidence bundle could not be opened or read to completion. This establishes no content
    /// mismatch; it is the companion to `EEvidenceIntegrity` for I/O/archive-read failures.
    EEvidenceUnreadable,
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
    /// Judge returned uncertain (abstain) — model could not decide; policy-dependent
    EJudgeUncertain,
    /// Policy violation detected
    EPolicyViolation,
    /// Sequence assertion failed
    ESequenceViolation,
    /// Argument schema validation failed
    EArgSchema,
}

/// The one construction of an executable recovery step that carries caller-controlled values.
///
/// Callers outside the registry render the same shape rather than building a second one, because
/// a shell string and a JSON argv disagree about what a value containing a quote or a space means.
pub(crate) fn format_recovery_argv(args: &[&str]) -> String {
    format!("Run argv: {}", serde_json::json!(args))
}

/// Classify an explicit `--config` that `load_config_with_cause` could not load.
///
/// Primary truth is the config-read I/O kind, set only at the `read_to_string`
/// fold. `NotFound` is absence. Every other kind, and a YAML/schema failure
/// with no kind, is unloadable. This is the one answer `run` and `doctor` share.
/// The public `ConfigError` tuple is not consulted.
pub(crate) fn reason_for_unloadable_explicit_config(err: &ConfigLoadError) -> ReasonCode {
    match err.io_kind() {
        Some(ErrorKind::NotFound) => ReasonCode::EMissingConfig,
        Some(_) | None => ReasonCode::ECfgParse,
    }
}

/// Bind a flag to a caller-controlled path as one argv element.
///
/// Always `flag=value`, including when `value` does not start with `-`. A split
/// `--flag` / value pair lets clap reread a `-prefixed` path as an option. A
/// conditional fuse would be a second place that rule could drift.
pub(crate) fn fused_option(flag: &str, value: &str) -> String {
    format!("{flag}={value}")
}

/// Owns `prefix + ["--", operand]` ordering for a positional recovery operand.
///
/// Decorating the value alone is not enough: placing the operand before `--`
/// lets clap reread a `-prefixed` path as an option, and a bundle named `--`
/// binds the wrong operand if the separator is not the last prefix element.
pub(crate) fn positional_operand<'a>(prefix: &'a [&'a str], operand: &'a str) -> Vec<&'a str> {
    let mut argv = Vec::with_capacity(prefix.len() + 2);
    argv.extend_from_slice(prefix);
    argv.push("--");
    argv.push(operand);
    argv
}

/// Bind a successful `init` recovery that validates the generated config.
///
/// Path-bearing flags are fused. `--format json` is always present so a consumer
/// that followed a JSON init report receives a JSON validate report. A replay
/// `--trace-file` is included only when that file is a runtime replay trace;
/// generator-event JSONL is not one.
pub(crate) fn validate_recovery_argv(config: &str, replay_trace: Option<&str>) -> Vec<String> {
    let mut argv = vec![
        "assay".to_string(),
        "validate".to_string(),
        fused_option("--config", config),
    ];
    if let Some(trace) = replay_trace {
        argv.push(fused_option("--trace-file", trace));
    }
    argv.push("--format".to_string());
    argv.push("json".to_string());
    argv
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
            | ReasonCode::EReplayLimitExceeded
            | ReasonCode::EReplayMissingDependency
            | ReasonCode::EEvidenceIntegrity
            | ReasonCode::EEvidenceContract
            | ReasonCode::EEvidenceUnreadable
            | ReasonCode::EInvalidArgs => EXIT_CONFIG_ERROR,

            // V2: Infra errors -> 3
            ReasonCode::EJudgeUnavailable
            | ReasonCode::ERateLimit
            | ReasonCode::EProvider5xx
            | ReasonCode::ETimeout
            | ReasonCode::ENetworkError => EXIT_INFRA_ERROR,

            // V2: Test failures -> 1
            ReasonCode::ETestFailed
            | ReasonCode::EJudgeUncertain
            | ReasonCode::EPolicyViolation
            | ReasonCode::ESequenceViolation
            | ReasonCode::EArgSchema => EXIT_TEST_FAILURE,
        }
    }

    fn exit_code_v1(&self) -> i32 {
        // Legacy mapping (V1)
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "the V1 compat mapping names the codes whose exit differed under V1 and lets the rest fall to their V2 code; a new code is V2-only until someone decides it had a V1 meaning"
        )]
        match self {
            ReasonCode::Success => EXIT_SUCCESS,

            // Keep replay missing-dependency deterministic across profiles.
            // This avoids compat-profile drift for the offline replay contract.
            ReasonCode::EReplayMissingDependency => EXIT_CONFIG_ERROR,

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
            ReasonCode::EReplayMissingDependency => "E_REPLAY_MISSING_DEPENDENCY",
            ReasonCode::EReplayLimitExceeded => "E_REPLAY_LIMIT_EXCEEDED",
            ReasonCode::EEvidenceIntegrity => "E_EVIDENCE_INTEGRITY",
            ReasonCode::EEvidenceContract => "E_EVIDENCE_CONTRACT",
            ReasonCode::EEvidenceUnreadable => "E_EVIDENCE_UNREADABLE",
            ReasonCode::EInvalidArgs => "E_INVALID_ARGS",
            ReasonCode::EJudgeUnavailable => "E_JUDGE_UNAVAILABLE",
            ReasonCode::ERateLimit => "E_RATE_LIMIT",
            ReasonCode::EProvider5xx => "E_PROVIDER_5XX",
            ReasonCode::ETimeout => "E_TIMEOUT",
            ReasonCode::ENetworkError => "E_NETWORK_ERROR",
            ReasonCode::ETestFailed => "E_TEST_FAILED",
            ReasonCode::EJudgeUncertain => "E_JUDGE_UNCERTAIN",
            ReasonCode::EPolicyViolation => "E_POLICY_VIOLATION",
            ReasonCode::ESequenceViolation => "E_SEQUENCE_VIOLATION",
            ReasonCode::EArgSchema => "E_ARG_SCHEMA",
        }
    }

    /// Suggested next step for this error.
    ///
    /// Executable recovery with caller-controlled values is JSON argv. Path-bearing
    /// flags are always fused (`--config=path`) so a `-prefixed` path remains an
    /// operand. Dynamic recoveries that produce a machine document include
    /// `--format json` here, not at the call site: one reason has one remediation,
    /// and a consumer that followed a JSON failure report can parse the result.
    /// The same string is published on the text channel. Dynamic trace-path
    /// guidance is prose rather than a command; remaining command-like steps are
    /// static strings and therefore carry no caller-controlled argument boundary.
    pub fn next_step(&self, context: Option<&str>) -> String {
        match self {
            ReasonCode::Success => String::new(),
            ReasonCode::ECfgParse => {
                let config = fused_option("--config", context.unwrap_or("<config.yaml>"));
                format_recovery_argv(&["assay", "doctor", &config, "--format", "json"])
            }
            ReasonCode::ETraceNotFound => {
                format!(
                    "Check trace file exists: {}",
                    context.unwrap_or("<trace.jsonl>")
                )
            }
            ReasonCode::EReplayLimitExceeded => {
                "Raise the replay ingest ceiling that was named, or supply a smaller bundle"
                    .to_string()
            }
            ReasonCode::EEvidenceIntegrity => {
                // Prose, not a command. Re-verifying the same bundle only repeats the same
                // failure, so publishing a verify invocation here would be a diagnostic dressed
                // as a remedy. Nothing this side of the producer can repair the content.
                "Obtain an undamaged bundle from its producer; the content this bundle carries \
                 does not match what it records"
                    .to_string()
            }
            ReasonCode::EEvidenceContract => {
                // Prose, not a command. Re-verifying the same bundle only repeats the same
                // format-contract failure. Conforming evidence has to come from the producer.
                "Obtain or reissue evidence that conforms to the declared bundle contract; \
                 this bundle was readable and does not satisfy that contract"
                    .to_string()
            }
            ReasonCode::EEvidenceUnreadable => {
                let argv = positional_operand(
                    &["assay", "evidence", "show", "--format", "json"],
                    context.unwrap_or("<bundle>"),
                );
                format_recovery_argv(&argv)
            }
            ReasonCode::EMissingConfig => "Run: assay init to create a config file".to_string(),
            ReasonCode::EBaselineInvalid => {
                "Run: assay baseline record to create a new baseline".to_string()
            }
            ReasonCode::EPolicyParse => {
                let input = fused_option("--input", context.unwrap_or("<policy.yaml>"));
                format_recovery_argv(&[
                    "assay",
                    "policy",
                    "validate",
                    &input,
                    "--format",
                    "json",
                ])
            }
            ReasonCode::EReplayMissingDependency => {
                "Replay bundle missing required offline dependency; rerun with --live or create a complete bundle".to_string()
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
            ReasonCode::EJudgeUncertain => {
                "Review borderline result or adjust judge threshold; run: assay explain <test-id>"
                    .to_string()
            }
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

    /// Create an outcome when judge returned uncertain (abstain) — exit 1, E_JUDGE_UNCERTAIN
    pub fn judge_uncertain(abstain_count: usize) -> Self {
        Self {
            exit_code: EXIT_TEST_FAILURE,
            reason_code: ReasonCode::EJudgeUncertain.as_str().to_string(),
            message: Some(format!(
                "Judge uncertain (abstain) for {} test(s); cannot decide pass/fail",
                abstain_count
            )),
            next_step: Some(
                "Review borderline result or adjust judge threshold; run: assay explain <test-id>"
                    .to_string(),
            ),
            warnings: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unloadable_explicit_config_class_follows_read_io_kind() {
        let cases = [
            (Some(ErrorKind::NotFound), ReasonCode::EMissingConfig),
            (Some(ErrorKind::PermissionDenied), ReasonCode::ECfgParse),
            (Some(ErrorKind::IsADirectory), ReasonCode::ECfgParse),
            (Some(ErrorKind::Other), ReasonCode::ECfgParse),
            (None, ReasonCode::ECfgParse),
        ];
        for (kind, expected) in cases {
            let err = match kind {
                Some(kind) => ConfigLoadError::from_read("failed to read config", kind),
                None => {
                    ConfigLoadError::new("failed to parse YAML: mapping values are not allowed")
                }
            };
            assert_eq!(
                reason_for_unloadable_explicit_config(&err),
                expected,
                "kind {kind:?} must not invent a more specific class"
            );
        }
    }

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
        assert_eq!(
            ReasonCode::EReplayMissingDependency.exit_code(),
            EXIT_CONFIG_ERROR
        );
        assert_eq!(ReasonCode::EInvalidArgs.exit_code(), EXIT_CONFIG_ERROR);
        assert_eq!(
            ReasonCode::EEvidenceIntegrity.exit_code(),
            EXIT_CONFIG_ERROR
        );
        assert_eq!(ReasonCode::EEvidenceContract.exit_code(), EXIT_CONFIG_ERROR);

        // Infra errors map to 3
        assert_eq!(ReasonCode::EJudgeUnavailable.exit_code(), EXIT_INFRA_ERROR);
        assert_eq!(ReasonCode::ERateLimit.exit_code(), EXIT_INFRA_ERROR);
        assert_eq!(ReasonCode::EProvider5xx.exit_code(), EXIT_INFRA_ERROR);
        assert_eq!(ReasonCode::ETimeout.exit_code(), EXIT_INFRA_ERROR);
        assert_eq!(ReasonCode::ENetworkError.exit_code(), EXIT_INFRA_ERROR);

        // Test failures map to 1
        assert_eq!(ReasonCode::ETestFailed.exit_code(), EXIT_TEST_FAILURE);
        assert_eq!(ReasonCode::EJudgeUncertain.exit_code(), EXIT_TEST_FAILURE);
        assert_eq!(ReasonCode::EPolicyViolation.exit_code(), EXIT_TEST_FAILURE);
        assert_eq!(
            ReasonCode::ESequenceViolation.exit_code(),
            EXIT_TEST_FAILURE
        );
        assert_eq!(ReasonCode::EArgSchema.exit_code(), EXIT_TEST_FAILURE);
    }

    #[test]
    fn test_replay_missing_dependency_profile_stability() {
        assert_eq!(
            ReasonCode::EReplayMissingDependency.exit_code_for(ExitCodeVersion::V1),
            EXIT_CONFIG_ERROR
        );
        assert_eq!(
            ReasonCode::EReplayMissingDependency.exit_code_for(ExitCodeVersion::V2),
            EXIT_CONFIG_ERROR
        );
    }

    #[test]
    fn evidence_integrity_holds_one_exit_class_across_profiles() {
        // The class is the same under both profiles by design, so a consumer that pins a
        // compatibility profile reads the same number. The V1 arm does not name this code, per the
        // documented rule that a new code is V2-only until someone decides it had a V1 meaning;
        // these assertions are what makes the intended equality checkable rather than incidental.
        assert_eq!(
            ReasonCode::EEvidenceIntegrity.exit_code_for(ExitCodeVersion::V1),
            EXIT_CONFIG_ERROR
        );
        assert_eq!(
            ReasonCode::EEvidenceIntegrity.exit_code_for(ExitCodeVersion::V2),
            EXIT_CONFIG_ERROR
        );
        assert_eq!(
            ReasonCode::EEvidenceUnreadable.exit_code_for(ExitCodeVersion::V1),
            EXIT_CONFIG_ERROR
        );
        assert_eq!(
            ReasonCode::EEvidenceUnreadable.exit_code_for(ExitCodeVersion::V2),
            EXIT_CONFIG_ERROR
        );
        assert_eq!(
            ReasonCode::EEvidenceContract.exit_code_for(ExitCodeVersion::V1),
            EXIT_CONFIG_ERROR
        );
        assert_eq!(
            ReasonCode::EEvidenceContract.exit_code_for(ExitCodeVersion::V2),
            EXIT_CONFIG_ERROR
        );
    }

    #[test]
    fn evidence_contract_remediation_promises_no_command() {
        let next_step = ReasonCode::EEvidenceContract.next_step(None);
        assert!(!next_step.is_empty(), "the remediation must not be empty");
        assert!(
            !next_step.starts_with("Run:") && !next_step.starts_with("Run argv:"),
            "contract remediation must stay prose, not an executable claim: {next_step}"
        );
        assert!(
            !next_step.contains("assay evidence verify"),
            "re-verifying the same bundle repeats the same failure, so naming that command \
             would publish a diagnostic as a remedy: {next_step}"
        );
        assert_eq!(
            next_step,
            ReasonCode::EEvidenceContract.next_step(Some("bundle; rm -rf /.tar.gz"))
        );
    }

    #[test]
    fn evidence_integrity_remediation_promises_no_command() {
        let next_step = ReasonCode::EEvidenceIntegrity.next_step(None);
        assert!(!next_step.is_empty(), "the remediation must not be empty");
        assert!(
            !next_step.starts_with("Run:") && !next_step.starts_with("Run argv:"),
            "integrity remediation must stay prose, not an executable claim: {next_step}"
        );
        assert!(
            !next_step.contains("assay evidence verify"),
            "re-verifying the same bundle repeats the same failure, so naming that command \
             would publish a diagnostic as a remedy: {next_step}"
        );
        // The context argument is the caller's path. Nothing is interpolated here, so a hostile
        // bundle path cannot reach the published string at all.
        assert_eq!(
            next_step,
            ReasonCode::EEvidenceIntegrity.next_step(Some("bundle; rm -rf /.tar.gz"))
        );
    }

    #[test]
    fn test_reason_code_as_str() {
        assert_eq!(ReasonCode::Success.as_str(), "");
        assert_eq!(ReasonCode::ECfgParse.as_str(), "E_CFG_PARSE");
        assert_eq!(ReasonCode::ETraceNotFound.as_str(), "E_TRACE_NOT_FOUND");
        assert_eq!(ReasonCode::EMissingConfig.as_str(), "E_MISSING_CONFIG");
        assert_eq!(ReasonCode::EBaselineInvalid.as_str(), "E_BASELINE_INVALID");
        assert_eq!(ReasonCode::EPolicyParse.as_str(), "E_POLICY_PARSE");
        assert_eq!(
            ReasonCode::EReplayMissingDependency.as_str(),
            "E_REPLAY_MISSING_DEPENDENCY"
        );
        assert_eq!(ReasonCode::EInvalidArgs.as_str(), "E_INVALID_ARGS");
        assert_eq!(
            ReasonCode::EEvidenceIntegrity.as_str(),
            "E_EVIDENCE_INTEGRITY"
        );
        assert_eq!(
            ReasonCode::EEvidenceContract.as_str(),
            "E_EVIDENCE_CONTRACT"
        );
        assert_eq!(
            ReasonCode::EEvidenceUnreadable.as_str(),
            "E_EVIDENCE_UNREADABLE"
        );
        assert_eq!(
            ReasonCode::EJudgeUnavailable.as_str(),
            "E_JUDGE_UNAVAILABLE"
        );
        assert_eq!(ReasonCode::ERateLimit.as_str(), "E_RATE_LIMIT");
        assert_eq!(ReasonCode::EProvider5xx.as_str(), "E_PROVIDER_5XX");
        assert_eq!(ReasonCode::ETimeout.as_str(), "E_TIMEOUT");
        assert_eq!(ReasonCode::ENetworkError.as_str(), "E_NETWORK_ERROR");
        assert_eq!(ReasonCode::ETestFailed.as_str(), "E_TEST_FAILED");
        assert_eq!(ReasonCode::EJudgeUncertain.as_str(), "E_JUDGE_UNCERTAIN");
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

        // Dynamic command recovery preserves hostile paths as one argv element.
        let config_path = "cfg file;$(touch should-not-exist).yaml";
        let config_next_step = ReasonCode::ECfgParse.next_step(Some(config_path));
        assert_eq!(
            config_next_step,
            r#"Run argv: ["assay","doctor","--config=cfg file;$(touch should-not-exist).yaml","--format","json"]"#,
            "the shared recovery formatter must preserve the published compact representation"
        );
        let config_argv: Vec<String> = serde_json::from_str(
            config_next_step
                .strip_prefix("Run argv: ")
                .expect("config recovery must publish JSON argv"),
        )
        .expect("config recovery argv must parse");
        assert_eq!(
            config_argv,
            vec![
                "assay".to_string(),
                "doctor".to_string(),
                format!("--config={config_path}"),
                "--format".to_string(),
                "json".to_string()
            ]
        );
        assert!(ReasonCode::ETraceNotFound
            .next_step(Some("traces/ci.jsonl"))
            .contains("traces/ci.jsonl"));
        assert!(ReasonCode::EMissingConfig
            .next_step(None)
            .contains("assay init"));
        assert!(ReasonCode::EBaselineInvalid
            .next_step(None)
            .contains("baseline"));
        let policy_path = "pol icy;$(echo x).yaml";
        let next_step = ReasonCode::EPolicyParse.next_step(Some(policy_path));
        let argv: Vec<String> = serde_json::from_str(
            next_step
                .strip_prefix("Run argv: ")
                .expect("policy recovery must publish JSON argv"),
        )
        .expect("policy recovery argv must parse");
        assert_eq!(
            argv,
            vec![
                "assay".to_string(),
                "policy".to_string(),
                "validate".to_string(),
                format!("--input={policy_path}"),
                "--format".to_string(),
                "json".to_string()
            ]
        );
        assert!(ReasonCode::EReplayMissingDependency
            .next_step(None)
            .contains("--live"));
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
        assert!(ReasonCode::EJudgeUncertain
            .next_step(None)
            .contains("borderline"));
        assert!(ReasonCode::EPolicyViolation
            .next_step(None)
            .contains("explain"));
        assert!(ReasonCode::ESequenceViolation
            .next_step(None)
            .contains("explain"));
        assert!(ReasonCode::EArgSchema.next_step(None).contains("explain"));
    }

    #[test]
    fn dynamic_recovery_argv_round_trips_json_significant_paths() {
        let path = "cfg \"quoted\"\\nested\nline\ttab\u{0007}.yaml";
        let cases = [
            (
                ReasonCode::ECfgParse,
                vec![
                    "assay".into(),
                    "doctor".into(),
                    format!("--config={path}"),
                    "--format".into(),
                    "json".into(),
                ],
            ),
            (
                ReasonCode::EPolicyParse,
                vec![
                    "assay".into(),
                    "policy".into(),
                    "validate".into(),
                    format!("--input={path}"),
                    "--format".into(),
                    "json".into(),
                ],
            ),
            (
                ReasonCode::EEvidenceUnreadable,
                vec![
                    "assay".into(),
                    "evidence".into(),
                    "show".into(),
                    "--format".into(),
                    "json".into(),
                    "--".into(),
                    path.into(),
                ],
            ),
        ];

        for (reason, expected) in cases {
            let next_step = reason.next_step(Some(path));
            let encoded = next_step
                .strip_prefix("Run argv: ")
                .expect("dynamic recovery must publish JSON argv");
            let argv: Vec<String> =
                serde_json::from_str(encoded).expect("recovery argv must remain valid JSON");
            assert_eq!(argv, expected, "{reason:?} must preserve one path argument");
        }
    }

    #[test]
    fn path_bearing_recovery_always_fuses_the_flag_and_includes_json_format() {
        for path in ["-weird.yaml", "--config", "-h", "plain.yaml"] {
            let config = ReasonCode::ECfgParse.next_step(Some(path));
            let policy = ReasonCode::EPolicyParse.next_step(Some(path));
            let config_argv: Vec<String> = serde_json::from_str(
                config
                    .strip_prefix("Run argv: ")
                    .expect("config recovery must publish JSON argv"),
            )
            .expect("config recovery argv must parse");
            let policy_argv: Vec<String> = serde_json::from_str(
                policy
                    .strip_prefix("Run argv: ")
                    .expect("policy recovery must publish JSON argv"),
            )
            .expect("policy recovery argv must parse");
            assert_eq!(
                config_argv,
                vec![
                    "assay".to_string(),
                    "doctor".to_string(),
                    format!("--config={path}"),
                    "--format".to_string(),
                    "json".to_string()
                ]
            );
            assert_eq!(
                policy_argv,
                vec![
                    "assay".to_string(),
                    "policy".to_string(),
                    "validate".to_string(),
                    format!("--input={path}"),
                    "--format".to_string(),
                    "json".to_string()
                ]
            );
            assert!(
                !config.contains('\u{0007}') && !policy.contains('\u{0007}'),
                "recovery text must not carry raw BEL"
            );
        }
    }

    fn publishes_recovery_argv(reason: ReasonCode) -> bool {
        match reason {
            ReasonCode::ECfgParse | ReasonCode::EPolicyParse | ReasonCode::EEvidenceUnreadable => {
                true
            }
            ReasonCode::Success
            | ReasonCode::ETraceNotFound
            | ReasonCode::EMissingConfig
            | ReasonCode::EBaselineInvalid
            | ReasonCode::EReplayMissingDependency
            | ReasonCode::EReplayLimitExceeded
            | ReasonCode::EEvidenceIntegrity
            | ReasonCode::EEvidenceContract
            | ReasonCode::EInvalidArgs
            | ReasonCode::EJudgeUnavailable
            | ReasonCode::ERateLimit
            | ReasonCode::EProvider5xx
            | ReasonCode::ETimeout
            | ReasonCode::ENetworkError
            | ReasonCode::ETestFailed
            | ReasonCode::EJudgeUncertain
            | ReasonCode::EPolicyViolation
            | ReasonCode::ESequenceViolation
            | ReasonCode::EArgSchema => false,
        }
    }

    #[test]
    fn every_reason_that_publishes_argv_is_a_known_executable_recovery() {
        let mut published = Vec::new();
        for reason in [
            ReasonCode::Success,
            ReasonCode::ECfgParse,
            ReasonCode::ETraceNotFound,
            ReasonCode::EMissingConfig,
            ReasonCode::EBaselineInvalid,
            ReasonCode::EPolicyParse,
            ReasonCode::EReplayMissingDependency,
            ReasonCode::EReplayLimitExceeded,
            ReasonCode::EEvidenceIntegrity,
            ReasonCode::EEvidenceContract,
            ReasonCode::EEvidenceUnreadable,
            ReasonCode::EInvalidArgs,
            ReasonCode::EJudgeUnavailable,
            ReasonCode::ERateLimit,
            ReasonCode::EProvider5xx,
            ReasonCode::ETimeout,
            ReasonCode::ENetworkError,
            ReasonCode::ETestFailed,
            ReasonCode::EJudgeUncertain,
            ReasonCode::EPolicyViolation,
            ReasonCode::ESequenceViolation,
            ReasonCode::EArgSchema,
        ] {
            let classified = publishes_recovery_argv(reason);
            let emitted = reason.next_step(Some("x")).starts_with("Run argv: ");
            assert_eq!(
                emitted, classified,
                "{reason:?}: classification and published next_step must agree"
            );
            if emitted {
                published.push(reason);
            }
        }
        assert_eq!(
            published,
            [
                ReasonCode::ECfgParse,
                ReasonCode::EPolicyParse,
                ReasonCode::EEvidenceUnreadable,
            ],
            "a new argv publisher must join the cross-publisher harness; dropping one is the skipped-variant mutation"
        );
    }

    #[test]
    fn positional_operand_owns_separator_before_the_value() {
        let prefix = ["assay", "evidence", "show", "--format", "json"];
        // `--` as the operand makes correct and swapped order byte-identical.
        let operand = "-bundle.tar.gz";
        let argv = positional_operand(&prefix, operand);
        assert_eq!(
            argv,
            vec![
                "assay",
                "evidence",
                "show",
                "--format",
                "json",
                "--",
                "-bundle.tar.gz"
            ]
        );
        let mut swapped = prefix.to_vec();
        swapped.push(operand);
        swapped.push("--");
        assert_ne!(
            argv, swapped,
            "operand `--` would make this assertion tautological"
        );
    }

    #[test]
    fn validate_recovery_argv_fuses_config_and_omits_generator_events() {
        assert_eq!(
            validate_recovery_argv("-weird.yaml", None),
            vec![
                "assay",
                "validate",
                "--config=-weird.yaml",
                "--format",
                "json"
            ]
        );
        assert_eq!(
            validate_recovery_argv("-weird.yaml", Some("traces/hello.jsonl")),
            vec![
                "assay",
                "validate",
                "--config=-weird.yaml",
                "--trace-file=traces/hello.jsonl",
                "--format",
                "json"
            ]
        );
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

    #[test]
    fn test_run_outcome_judge_uncertain() {
        let outcome = RunOutcome::judge_uncertain(1);
        assert_eq!(outcome.exit_code, EXIT_TEST_FAILURE);
        assert_eq!(outcome.reason_code, "E_JUDGE_UNCERTAIN");
        assert!(outcome.message.as_ref().unwrap().contains("uncertain"));
        assert!(outcome.next_step.as_ref().unwrap().contains("borderline"));
    }
}
