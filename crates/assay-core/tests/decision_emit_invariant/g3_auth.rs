use crate::fixtures::{make_tool_request, TestEmitter};
use assay_core::mcp::g3_auth_context::AuthContextProjection;
use assay_core::mcp::policy::{McpPolicy, PolicyState};
use assay_core::mcp::tool_call_handler::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use serde_json::json;
use std::sync::Arc;

/// Synthetic JWS-shaped string for redaction tests (not a real credential).
const SYNTHETIC_JWS_COMPACT: &str =
    "eyJxxxxxxxxxxxxxxxxxxxx.yyyyyyyyyyyyyyyyyyyyyyyy.zzzzzzzzzzzzzzzzzzzzzzzz";

#[test]
fn g3_auth_projection_emits_allowlisted_scheme_trimmed_issuer_principal_in_decision_json() {
    let emitter = Arc::new(TestEmitter::new());
    let policy = McpPolicy::default();
    let config = ToolCallHandlerConfig {
        auth_context_projection: Some(AuthContextProjection {
            auth_scheme: Some("  OAUTH2  ".to_string()),
            auth_issuer: Some("  https://issuer.example/realms/acme  ".to_string()),
            principal: Some("alice@corp".to_string()),
        }),
        ..Default::default()
    };
    let handler = ToolCallHandler::new(policy, None, emitter.clone(), config);

    let request = make_tool_request("safe_tool");
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);
    assert!(matches!(result, HandleResult::Allow { .. }));

    let event = emitter.last_event().expect("event");
    let v = serde_json::to_value(&event.data).expect("serde data");
    assert_eq!(v["auth_scheme"], json!("oauth2"));
    assert_eq!(
        v["auth_issuer"],
        json!("https://issuer.example/realms/acme")
    );
    assert_eq!(v["principal"], json!("alice@corp"));

    let wire = serde_json::to_string(&event).expect("wire");
    assert!(
        wire.contains("\"auth_scheme\":\"oauth2\""),
        "emitted JSON must carry normalized auth_scheme"
    );
}

#[test]
fn g3_unknown_auth_scheme_dropped_whitespace_principal_absent_in_decision_json() {
    let emitter = Arc::new(TestEmitter::new());
    let policy = McpPolicy::default();
    let config = ToolCallHandlerConfig {
        auth_context_projection: Some(AuthContextProjection {
            auth_scheme: Some("openid".to_string()),
            auth_issuer: Some("https://issuer.ok".to_string()),
            principal: Some("  \n\t  ".to_string()),
        }),
        ..Default::default()
    };
    let handler = ToolCallHandler::new(policy, None, emitter.clone(), config);

    let request = make_tool_request("safe_tool");
    let mut state = PolicyState::default();
    let _ = handler.handle_tool_call(&request, &mut state, None, None, None);

    let event = emitter.last_event().expect("event");
    let v = serde_json::to_value(&event.data).expect("serde data");
    assert!(
        v.get("auth_scheme").is_none(),
        "unknown scheme must not emit"
    );
    assert_eq!(v["auth_issuer"], json!("https://issuer.ok"));
    assert!(
        v.get("principal").is_none(),
        "whitespace-only principal absent"
    );
}

#[test]
fn g3_jwt_and_bearer_material_never_appear_on_emitted_decision_json() {
    let emitter = Arc::new(TestEmitter::new());
    let policy = McpPolicy::default();
    let config = ToolCallHandlerConfig {
        auth_context_projection: Some(AuthContextProjection {
            auth_scheme: Some("jwt_bearer".to_string()),
            auth_issuer: Some(SYNTHETIC_JWS_COMPACT.to_string()),
            principal: Some(format!("Bearer {}", SYNTHETIC_JWS_COMPACT)),
        }),
        ..Default::default()
    };
    let handler = ToolCallHandler::new(policy, None, emitter.clone(), config);

    let request = make_tool_request("safe_tool");
    let mut state = PolicyState::default();
    let _ = handler.handle_tool_call(&request, &mut state, None, None, None);

    let event = emitter.last_event().expect("event");
    let wire = serde_json::to_string(&event).expect("wire");
    assert!(
        !wire.contains(SYNTHETIC_JWS_COMPACT),
        "JWS-shaped credential material must not appear on the wire"
    );
    assert!(
        !wire.to_ascii_lowercase().contains("bearer "),
        "bearer credential material must not appear"
    );
    let v = serde_json::to_value(&event.data).expect("serde data");
    assert_eq!(v["auth_scheme"], json!("jwt_bearer"));
    assert!(v.get("auth_issuer").is_none());
    assert!(v.get("principal").is_none());
}
