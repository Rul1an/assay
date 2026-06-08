//! Argument-constraint enforcement over the shipped `policy_engine` (JSON-Schema backed).
//!
//! Anchors the E7 frontier finding at the unit level: a CaMeL-style data-flow gate and an
//! effect/counterparty boundary cover different attacks, and the gap between them is exactly an
//! argument constraint. `send_money(recipient, amount)` shows the three relevant points:
//!
//!   - amount above the ceiling, recipient already declared  -> the counterparty boundary ALLOWS
//!     (right recipient) but the `maximum` schema BLOCKS. This is the region the boundary misses.
//!   - recipient outside the allowlist                       -> the `enum` schema BLOCKS.
//!   - legitimate in-bounds amount to a declared recipient   -> ALLOWED, even when the amount value
//!     coincides with untrusted text. An argument constraint has no taint notion, so it does not
//!     over-block the way a data-flow gate does on a legitimately derived value.
//!
//! All three verdicts come from the real `assay_core::policy_engine::evaluate_tool_args`, i.e. the
//! same `jsonschema` validator the `args_valid` metric and the MCP policy use. No mock.

use assay_core::policy_engine::{evaluate_tool_args, VerdictStatus};
use serde_json::json;

/// Banking args-constraint policy: an amount ceiling and a recipient allowlist for `send_money`.
fn banking_policy() -> serde_json::Value {
    json!({
        "send_money": {
            "type": "object",
            "properties": {
                "recipient": { "type": "string", "enum": ["acc_alice", "acc_bob"] },
                "amount": { "type": "number", "maximum": 1000 }
            },
            "required": ["recipient", "amount"]
        }
    })
}

#[test]
fn amount_ceiling_blocks_in_recipient_overbudget_transfer() {
    // recipient is declared (the counterparty boundary would ALLOW this), but the amount is tampered
    // above the ceiling. This is precisely the gap an effect/counterparty boundary leaves open.
    let policy = banking_policy();
    let v = evaluate_tool_args(
        &policy,
        "send_money",
        &json!({ "recipient": "acc_alice", "amount": 10_000 }),
    );
    assert_eq!(
        v.status,
        VerdictStatus::Blocked,
        "over-ceiling amount must block"
    );
    assert_eq!(v.reason_code, "E_ARG_SCHEMA");
}

#[test]
fn recipient_allowlist_blocks_undeclared_counterparty() {
    let policy = banking_policy();
    let v = evaluate_tool_args(
        &policy,
        "send_money",
        &json!({ "recipient": "acc_attacker", "amount": 100 }),
    );
    assert_eq!(
        v.status,
        VerdictStatus::Blocked,
        "off-allowlist recipient must block"
    );
    assert_eq!(v.reason_code, "E_ARG_SCHEMA");
}

#[test]
fn legitimate_in_bounds_transfer_is_allowed_no_overblock() {
    // In-bounds amount to a declared recipient. The argument constraint has no taint notion, so even
    // if "100" appeared verbatim in untrusted tool output (where a data-flow gate would over-block),
    // the constraint allows it. This is the utility the combined architecture preserves.
    let policy = banking_policy();
    let v = evaluate_tool_args(
        &policy,
        "send_money",
        &json!({ "recipient": "acc_alice", "amount": 100 }),
    );
    assert_eq!(
        v.status,
        VerdictStatus::Allowed,
        "valid in-bounds transfer must pass"
    );
    assert_eq!(v.reason_code, "OK");
}

#[test]
fn ceiling_boundary_is_inclusive() {
    // `maximum` is inclusive: exactly at the ceiling passes, one above blocks. Pins the boundary so
    // the demonstration's numbers are not coincidental.
    let policy = banking_policy();
    let at = evaluate_tool_args(
        &policy,
        "send_money",
        &json!({ "recipient": "acc_bob", "amount": 1000 }),
    );
    assert_eq!(
        at.status,
        VerdictStatus::Allowed,
        "amount == maximum must pass"
    );

    let over = evaluate_tool_args(
        &policy,
        "send_money",
        &json!({ "recipient": "acc_bob", "amount": 1001 }),
    );
    assert_eq!(
        over.status,
        VerdictStatus::Blocked,
        "amount == maximum + 1 must block"
    );
}
