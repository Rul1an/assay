//! `PolicyState` (compile-once) must produce verdicts identical to the one-shot `evaluate_tool_args`,
//! so switching a hot loop to compile-once changes performance, never behaviour.

use assay_core::policy_engine::{evaluate_tool_args, PolicyState, VerdictStatus};
use serde_json::json;

fn policy() -> serde_json::Value {
    json!({
        "send_money": {
            "type": "object",
            "properties": {
                "recipient": { "type": "string", "enum": ["acc_alice"] },
                "amount": { "type": "number", "maximum": 1000 }
            },
            "required": ["recipient", "amount"]
        },
        // an intentionally broken schema for one tool, to exercise E_SCHEMA_COMPILE parity
        "broken": { "type": "not-a-real-type" }
    })
}

fn assert_parity(tool: &str, args: &serde_json::Value) {
    let p = policy();
    let state = PolicyState::compile(&p);
    let cached = state.evaluate(tool, args);
    let one_shot = evaluate_tool_args(&p, tool, args);
    assert_eq!(cached.status, one_shot.status, "status differs for {tool}");
    assert_eq!(
        cached.reason_code, one_shot.reason_code,
        "reason_code differs for {tool}"
    );
}

#[test]
fn allowed_call_parity() {
    assert_parity(
        "send_money",
        &json!({ "recipient": "acc_alice", "amount": 100 }),
    );
}

#[test]
fn blocked_over_ceiling_parity() {
    let p = policy();
    let v = PolicyState::compile(&p).evaluate(
        "send_money",
        &json!({ "recipient": "acc_alice", "amount": 9999 }),
    );
    assert_eq!(v.status, VerdictStatus::Blocked);
    assert_eq!(v.reason_code, "E_ARG_SCHEMA");
    assert_parity(
        "send_money",
        &json!({ "recipient": "acc_alice", "amount": 9999 }),
    );
}

#[test]
fn off_allowlist_recipient_parity() {
    assert_parity(
        "send_money",
        &json!({ "recipient": "acc_attacker", "amount": 10 }),
    );
}

#[test]
fn missing_tool_parity() {
    // A tool not in the policy: both paths return E_POLICY_MISSING_TOOL (incl. the did-you-mean path).
    let p = policy();
    let v = PolicyState::compile(&p).evaluate("send_monye", &json!({}));
    assert_eq!(v.status, VerdictStatus::Blocked);
    assert_eq!(v.reason_code, "E_POLICY_MISSING_TOOL");
    assert_parity("send_monye", &json!({}));
}

#[test]
fn broken_schema_only_surfaces_when_that_tool_is_evaluated() {
    let p = policy();
    let state = PolicyState::compile(&p);
    // The broken schema is compiled eagerly but must not affect an unrelated tool's verdict.
    let ok = state.evaluate(
        "send_money",
        &json!({ "recipient": "acc_alice", "amount": 1 }),
    );
    assert_eq!(ok.status, VerdictStatus::Allowed);
    // Evaluating the broken tool surfaces the compile error, same as the one-shot path.
    let broken = state.evaluate("broken", &json!({}));
    assert_eq!(broken.status, VerdictStatus::Blocked);
    assert_eq!(broken.reason_code, "E_SCHEMA_COMPILE");
    assert_parity("broken", &json!({}));
}

#[test]
fn compile_once_reused_across_many_calls() {
    // The point of the cache: one compile, many evaluations, stable verdicts.
    let p = policy();
    let state = PolicyState::compile(&p);
    for _ in 0..50 {
        assert_eq!(
            state
                .evaluate(
                    "send_money",
                    &json!({ "recipient": "acc_alice", "amount": 100 })
                )
                .status,
            VerdictStatus::Allowed
        );
        assert_eq!(
            state
                .evaluate(
                    "send_money",
                    &json!({ "recipient": "acc_alice", "amount": 5000 })
                )
                .status,
            VerdictStatus::Blocked
        );
    }
}
