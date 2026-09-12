//! Fuzz the `tools/call` decision inputs through the real policy-decision
//! functions, not copies: the envelope classifier
//! (`assay_mcp_server::tools::classify_call_tool_params`) and the observed
//! tool-decision builder (`assay_mcp_server::tool_decision`).
//!
//! Two modes over one input. The envelope mode reads the whole input as a
//! `tools/call` params value; non-JSON inputs exercise the malformed-call
//! arm, and that arm is total too. The direct mode splits the input into a
//! tool name (first line) and arguments (the rest as JSON, `{}` when it does
//! not parse), which reaches the privileged-action classifier branches that
//! no advertised `assay_*` tool name can spell. Both modes assert the same
//! oracle: the decision is total (every input yields a classified outcome
//! with a machine-readable reason code, never a panic) and deterministic
//! (same input, same record), and arguments stay redacted no matter how
//! hostile they are.

#![no_main]

use assay_mcp_server::tool_decision::{build_decision, classify, Effect, ObservedCall};
use assay_mcp_server::tools::classify_call_tool_params;
use libfuzzer_sys::fuzz_target;
use serde_json::Value;

fn effect_name(effect: Effect) -> &'static str {
    match effect {
        Effect::Allow => "allow",
        Effect::Deny => "deny",
        Effect::Error => "error",
    }
}

/// Totality + determinism + redaction for one classified decision input.
fn assert_decision_sound(tool_name: &str, args: &Value) {
    // The privileged-action classifier is total on its own: every
    // (tool name, arguments) pair yields a state with a reason code.
    let first = classify(tool_name, args);
    let second = classify(tool_name, args);
    assert_eq!(first.state, second.state);
    assert_eq!(first.reason_code, second.reason_code);
    assert_eq!(first.class, second.class);
    assert_eq!(first.target, second.target);
    assert!(!first.reason_code.is_empty());

    for effect in [Effect::Allow, Effect::Deny, Effect::Error] {
        let status = match effect {
            Effect::Allow => "success",
            Effect::Deny => "blocked",
            Effect::Error => "error",
        };
        let call = ObservedCall {
            server_id: "fuzz",
            tool_name,
            args,
            effect,
            status,
            rule_id: None,
            traceparent: None,
        };
        let first = build_decision(&call);
        let second = build_decision(&call);
        // Deterministic: same input, same decision record.
        assert_eq!(first, second);
        // Total: the effect echoes the observed outcome ...
        assert_eq!(
            first.pointer("/decision/effect").and_then(Value::as_str),
            Some(effect_name(effect))
        );
        // ... with a classification and reason code on every input ...
        assert!(
            first
                .get("reason_code")
                .and_then(Value::as_str)
                .is_some_and(|reason| !reason.is_empty())
        );
        // ... and arguments stay redacted no matter how hostile they are.
        assert_eq!(
            first.pointer("/redaction/arguments_redacted"),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            first.pointer("/redaction/secret_material_stored"),
            Some(&Value::Bool(false))
        );
    }
}

fuzz_target!(|data: &[u8]| {
    let text = match core::str::from_utf8(data) {
        Ok(text) => text,
        Err(_) => return,
    };

    // Envelope mode: the whole input is one `tools/call` params value.
    let params: Option<Value> = serde_json::from_str(text).ok();
    if let Ok(dispatch) = classify_call_tool_params(params.as_ref()) {
        assert_decision_sound(&dispatch.name, &dispatch.arguments);
    } else {
        // Total: fixed code, fixed message, fixed data shape on every input.
        let fault = classify_call_tool_params(params.as_ref()).unwrap_err();
        assert_eq!(fault.code(), -32602);
        assert!(!fault.message().is_empty());
        assert!(fault.data().is_object());
    }

    // Direct mode: first line names the tool, the rest is its arguments.
    // Truncated so one input cannot buy unbounded hashing work.
    let mut lines = text.splitn(2, '\n');
    let tool_name: String = lines.next().unwrap_or("").chars().take(256).collect();
    let args: Value = lines
        .next()
        .and_then(|rest| serde_json::from_str(rest).ok())
        .filter(|args: &Value| args.is_object())
        .unwrap_or(Value::Null);
    assert_decision_sound(&tool_name, &args);
});
