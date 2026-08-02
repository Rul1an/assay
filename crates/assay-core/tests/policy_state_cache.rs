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
    assert_eq!(cached, one_shot, "verdict differs for {tool}");
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

#[test]
fn shared_defs_have_one_shot_and_cached_parity() {
    let p = json!({
        "$defs": {
            "safe_path": {
                "type": "string",
                "pattern": "^/workspace/"
            }
        },
        "read_file": {
            "type": "object",
            "properties": {
                "path": {"$ref": "#/$defs/safe_path"}
            },
            "required": ["path"]
        }
    });
    let state = PolicyState::compile(&p);

    for (args, expected) in [
        (json!({"path": "/workspace/a"}), VerdictStatus::Allowed),
        (json!({"path": "/tmp/a"}), VerdictStatus::Blocked),
    ] {
        let cached = state.evaluate("read_file", &args);
        let one_shot = evaluate_tool_args(&p, "read_file", &args);
        assert_eq!(cached.status, expected);
        assert_eq!(one_shot.status, expected);
        assert_eq!(cached.reason_code, one_shot.reason_code);
    }
}

#[test]
fn external_file_refs_are_not_retrieved() {
    let dir = tempfile::tempdir().expect("tempdir");
    let schema_path = dir.path().join("external.json");
    std::fs::write(&schema_path, r#"{"type":"string"}"#).expect("write external schema");
    let external_ref = url::Url::from_file_path(&schema_path)
        .expect("absolute path becomes file URL")
        .to_string();
    let p = json!({"read_file": {"$ref": external_ref}});

    let one_shot = evaluate_tool_args(&p, "read_file", &json!("accepted before the fix"));
    let cached = PolicyState::compile(&p).evaluate("read_file", &json!("accepted before the fix"));

    assert_eq!(one_shot.reason_code, "E_SCHEMA_COMPILE");
    assert_eq!(cached.reason_code, "E_SCHEMA_COMPILE");
    assert!(
        one_shot.details["message"]
            .as_str()
            .is_some_and(|message| message.contains("external JSON Schema retrieval is disabled")),
        "{}",
        one_shot.details
    );
}

#[test]
fn ref_shaped_instance_data_does_not_trigger_external_retrieval() {
    let p = json!({
        "record": {
            "const": {"$ref": "https://example.invalid/not-a-schema-reference"}
        }
    });
    let args = json!({"$ref": "https://example.invalid/not-a-schema-reference"});

    assert_eq!(
        evaluate_tool_args(&p, "record", &args).status,
        VerdictStatus::Allowed
    );
    assert_eq!(
        PolicyState::compile(&p).evaluate("record", &args).status,
        VerdictStatus::Allowed
    );
}

#[test]
fn absolute_same_document_refs_remain_hermetic() {
    let p = json!({
        "lookup": {
            "$id": "https://example.invalid/schemas/lookup",
            "$defs": {"identifier": {"type": "string"}},
            "$ref": "https://example.invalid/schemas/lookup#/$defs/identifier"
        }
    });

    assert_parity_for_policy(&p, "lookup", &json!("id-1"));
    assert_eq!(
        evaluate_tool_args(&p, "lookup", &json!("id-1")).status,
        VerdictStatus::Allowed
    );
}

#[test]
fn shared_defs_use_the_consuming_schema_dialect() {
    let p = json!({
        "$defs": {
            "positive": {"type": "number", "minimum": 0, "exclusiveMinimum": true}
        },
        "legacy": {
            "$schema": "http://json-schema.org/draft-04/schema#",
            "$ref": "#/$defs/positive"
        }
    });

    assert_parity_for_policy(&p, "legacy", &json!(1));
    assert_eq!(
        evaluate_tool_args(&p, "legacy", &json!(1)).status,
        VerdictStatus::Allowed
    );
}

#[test]
fn one_tool_preparation_error_does_not_poison_siblings() {
    let p = json!({
        "$defs": {"identifier": {"type": "string"}},
        "safe": {"type": "object"},
        "colliding": {
            "$defs": {"identifier": {"type": "integer"}},
            "$ref": "#/$defs/identifier"
        }
    });
    let state = PolicyState::compile(&p);

    assert_eq!(
        evaluate_tool_args(&p, "safe", &json!({})).status,
        VerdictStatus::Allowed
    );
    assert_eq!(
        state.evaluate("safe", &json!({})).status,
        VerdictStatus::Allowed
    );
    assert_eq!(
        evaluate_tool_args(&p, "colliding", &json!(1)).reason_code,
        "E_SCHEMA_COMPILE"
    );
    assert_eq!(
        state.evaluate("colliding", &json!(1)).reason_code,
        "E_SCHEMA_COMPILE"
    );
}

#[test]
fn shared_and_tool_local_defs_are_merged_without_overwriting() {
    let p = json!({
        "$defs": {
            "safe_path": {"type": "string", "pattern": "^/workspace/"}
        },
        "read_file": {
            "$defs": {
                "safe_mode": {"type": "string", "enum": ["read"]}
            },
            "type": "object",
            "properties": {
                "path": {"$ref": "#/$defs/safe_path"},
                "mode": {"$ref": "#/$defs/safe_mode"}
            },
            "required": ["path", "mode"]
        }
    });

    let state = PolicyState::compile(&p);
    assert_eq!(
        state
            .evaluate(
                "read_file",
                &json!({"path": "/workspace/a", "mode": "read"})
            )
            .status,
        VerdictStatus::Allowed
    );
    assert_eq!(
        state
            .evaluate("read_file", &json!({"path": "/tmp/a", "mode": "read"}))
            .status,
        VerdictStatus::Blocked
    );
    assert_eq!(
        state
            .evaluate(
                "read_file",
                &json!({"path": "/workspace/a", "mode": "write"})
            )
            .status,
        VerdictStatus::Blocked
    );
    assert_parity_for_policy(
        &p,
        "read_file",
        &json!({"path": "/workspace/a", "mode": "read"}),
    );
}

#[test]
fn missing_tool_details_ignore_the_reserved_defs_entry_on_both_paths() {
    let p = json!({
        "$defs": {"shared": {"type": "string"}},
        "read_file": {"type": "object"}
    });
    let one_shot = evaluate_tool_args(&p, "$def", &json!({}));
    let cached = PolicyState::compile(&p).evaluate("$def", &json!({}));

    assert_eq!(one_shot, cached);
    assert!(!one_shot.details.to_string().contains("$defs"));
}

fn assert_parity_for_policy(policy: &serde_json::Value, tool: &str, args: &serde_json::Value) {
    let cached = PolicyState::compile(policy).evaluate(tool, args);
    let one_shot = evaluate_tool_args(policy, tool, args);
    assert_eq!(cached, one_shot);
}
