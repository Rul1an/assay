//! Fuzz the `tools/call` decision inputs through the real policy-decision
//! functions, not copies: the envelope classifier
//! (`assay_mcp_server::tools::classify_call_tool_params`), the production
//! `handle_call` evaluator, the observed-effect derivation
//! (`assay_mcp_server::tool_decision::observed_effect`), and the observed
//! tool-decision builder (`assay_mcp_server::tool_decision`).
//!
//! Two classification modes over one input. The envelope mode reads the whole
//! input as a `tools/call` params value; non-JSON inputs exercise the
//! malformed-call arm, and that arm is total too. The direct mode splits the
//! input into a tool name (first line) and arguments (the rest as JSON, `{}`
//! when it does not parse), which reaches the privileged-action classifier
//! branches that no advertised `assay_*` tool name can spell. Both modes
//! assert the same classification oracle: the decision is total (every input
//! yields a classified outcome with a machine-readable reason code, never a
//! panic) and deterministic (same input, same record), and arguments stay
//! redacted no matter how hostile they are.
//!
//! Envelope-classified known tools also run `handle_call` against a process-
//! lifetime policy fixture, then derive the effect the same way `Server::run`
//! does. That path asserts totality, determinism of the real outcome, and
//! that the decision record built from that outcome is well-formed.

#![no_main]

use assay_mcp_server::cache::PolicyCaches;
use assay_mcp_server::config::ServerConfig;
use assay_mcp_server::tool_decision::{
    build_decision, classify, observed_effect, traceparent_from_params, Effect, ObservedCall,
};
use assay_mcp_server::tools::{classify_call_tool_params, handle_call, ToolContext, ToolError};
use libfuzzer_sys::fuzz_target;
use serde_json::Value;
use std::sync::OnceLock;

fn effect_name(effect: Effect) -> &'static str {
    match effect {
        Effect::Allow => "allow",
        Effect::Deny => "deny",
        Effect::Error => "error",
    }
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

/// Process-lifetime policy fixture. `_dir` is stored next to `ctx` so
/// `TempDir`'s Drop does not delete the policy files while the context
/// still points at them.
struct Fixture {
    _dir: tempfile::TempDir,
    ctx: ToolContext,
}

/// One fixture policy for the process: `blocked_tool` and `Blocked` are denied;
/// every other name is allowed. Missing files stay missing so Error is reachable.
fn fixture_ctx() -> &'static ToolContext {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    &FIXTURE
        .get_or_init(|| {
            let dir = tempfile::tempdir().expect("fixture policy dir");
            std::fs::write(
                dir.path().join("policy.yaml"),
                "blocklist:\n  - blocked_tool\n  - Blocked\n",
            )
            .expect("fixture policy");
            let policy_root = dir.path().to_path_buf();
            let canon = std::fs::canonicalize(&policy_root).expect("fixture policy canon");
            Fixture {
                ctx: ToolContext {
                    policy_root,
                    policy_root_canon: canon,
                    cfg: ServerConfig::default(),
                    caches: PolicyCaches::new(128),
                },
                _dir: dir,
            }
        })
        .ctx
}

/// The same fail-closed construction `Server::run` uses for `handle_call` Err:
/// `ToolError::new(...).result()`. The message literal is the remaining seam;
/// the constant that names it in `server.rs` is private.
fn fail_closed_internal() -> Value {
    ToolError::new("E_INTERNAL", "Tool execution failed")
        .result()
        .expect("fail-closed ToolError serializes")
}

fn execute_handle_call(ctx: &ToolContext, name: &str, args: &Value) -> Value {
    match runtime().block_on(handle_call(ctx, name, args)) {
        Ok(value) => value,
        Err(_) => fail_closed_internal(),
    }
}

/// Totality + determinism + redaction for one classified decision input.
fn assert_well_formed_decision(call: &ObservedCall<'_>) {
    let first = build_decision(call);
    let second = build_decision(call);
    // Deterministic: same input, same decision record.
    assert_eq!(first, second);
    // Total: the effect echoes the observed outcome ...
    assert_eq!(
        first.pointer("/decision/effect").and_then(Value::as_str),
        Some(effect_name(call.effect))
    );
    // ... with a classification and reason code on every input ...
    assert!(first
        .get("reason_code")
        .and_then(Value::as_str)
        .is_some_and(|reason| !reason.is_empty()));
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
        assert_well_formed_decision(&call);
    }
}

fn assert_evaluated_decision(params: &Value, tool_name: &str, args: &Value) {
    let ctx = fixture_ctx();
    let first_result = execute_handle_call(ctx, tool_name, args);
    let second_result = execute_handle_call(ctx, tool_name, args);
    assert_eq!(first_result, second_result);

    let (effect, status) = observed_effect(&first_result);
    let (effect2, status2) = observed_effect(&second_result);
    assert_eq!(effect, effect2);
    assert_eq!(status, status2);

    // Restate the derivation rule independently of `observed_effect` so a
    // mutant that maps an error result to Allow fails here (must-bite M2).
    if first_result
        .get("error")
        .and_then(|err| err.get("code"))
        .and_then(Value::as_str)
        .is_some()
    {
        assert_eq!(effect, Effect::Error);
    } else if first_result.get("allowed").and_then(Value::as_bool) == Some(true) {
        assert_eq!(effect, Effect::Allow);
    } else {
        assert_eq!(effect, Effect::Deny);
    }

    // Fixture policy outcomes so a mutant that allows a denied tool fails
    // (must-bite M1). Only exact seed shapes are pinned; mutated inputs may
    // hit limits or other tools.
    if tool_name == "assay_policy_decide" {
        let tool = args.get("tool").and_then(Value::as_str);
        let policy = args.get("policy").and_then(Value::as_str);
        match (tool, policy) {
            (Some("blocked_tool") | Some("Blocked"), Some("policy.yaml")) => {
                assert_eq!(effect, Effect::Deny);
            }
            (Some("allowed_tool"), Some("policy.yaml")) => {
                assert_eq!(effect, Effect::Allow);
            }
            (_, Some("missing.yaml")) => {
                assert_eq!(effect, Effect::Error);
            }
            _ => {}
        }
    }

    let call = ObservedCall {
        server_id: "fuzz",
        tool_name,
        args,
        effect,
        status: &status,
        rule_id: None,
        traceparent: traceparent_from_params(params),
    };
    assert_well_formed_decision(&call);
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
        if let Some(params) = params.as_ref() {
            assert_evaluated_decision(params, &dispatch.name, &dispatch.arguments);
        }
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
