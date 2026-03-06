use crate::assertions::compute_result_hash;
use crate::{CheckInput, CheckResult, CheckType, PolicyCheck};

/// Streaming mode evaluation (simulates `assay-mcp-server`)
pub fn evaluate(check: &PolicyCheck, input: &CheckInput) -> CheckResult {
    // This simulates the streaming/preflight evaluation path
    // In real implementation, this would call the MCP server handler

    // CRITICAL: Must use IDENTICAL logic to batch mode
    // The only difference should be the execution context, not the logic

    let (outcome, reason) = match check.check_type {
        CheckType::ArgsValid => evaluate_args_valid(&check.params, input),
        CheckType::SequenceValid => evaluate_sequence_valid(&check.params, input),
        CheckType::ToolBlocklist => evaluate_blocklist(&check.params, input),
    };

    let result_hash = compute_result_hash(&check.id, &outcome, &reason);

    CheckResult {
        check_id: check.id.clone(),
        outcome,
        reason,
        result_hash,
    }
}

// These functions MUST be identical to batch mode
// In production, both modes should call the same underlying functions
// from assay-metrics

fn evaluate_args_valid(params: &serde_json::Value, input: &CheckInput) -> (crate::Outcome, String) {
    // Delegate to shared implementation
    crate::shared::args_valid(params, input)
}

fn evaluate_sequence_valid(
    params: &serde_json::Value,
    input: &CheckInput,
) -> (crate::Outcome, String) {
    crate::shared::sequence_valid(params, input)
}

fn evaluate_blocklist(params: &serde_json::Value, input: &CheckInput) -> (crate::Outcome, String) {
    crate::shared::blocklist(params, input)
}
