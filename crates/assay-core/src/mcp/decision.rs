//! Tool decision events and always-emit guard (SPEC-Mandate-v1.0.4 §7.9).
//!
//! This module implements the "always emit decision" invariant (I1):
//! Every tool call attempt MUST emit exactly one decision event.

#[path = "decision_next/mod.rs"]
mod decision_next;

mod consumer_contract;
mod context_contract;
mod deny_convergence;
mod outcome_convergence;
mod replay_compat;
mod replay_diff;

pub use self::decision_next::emitters::{
    DecisionEmitter, FileDecisionEmitter, NullDecisionEmitter,
};
pub use self::decision_next::event_types::{
    reason_codes, Decision, DecisionData, DecisionEvent, FulfillmentDecisionPath,
    ObligationOutcome, ObligationOutcomeStatus, PolicyDecisionEventContext,
    POLICY_SNAPSHOT_CANONICALIZATION_JCS_MCP_POLICY, POLICY_SNAPSHOT_DIGEST_ALG_SHA256,
    POLICY_SNAPSHOT_SCHEMA_V1,
};
pub use self::decision_next::guard::DecisionEmitterGuard;
pub(crate) use self::decision_next::normalization::refresh_contract_projections;
pub use consumer_contract::{
    required_consumer_fields_v1, ConsumerPayloadState, ConsumerReadPath,
    DECISION_CONSUMER_CONTRACT_VERSION_V1,
};
pub use context_contract::{
    required_context_fields_v1, ContextPayloadState, DECISION_CONTEXT_CONTRACT_VERSION_V1,
};
pub use deny_convergence::{DenyClassificationSource, DENY_PRECEDENCE_VERSION_V1};
pub use outcome_convergence::{DecisionOrigin, DecisionOutcomeKind, OutcomeCompatState};
pub use replay_compat::{ReplayClassificationSource, DECISION_BASIS_VERSION_V1};
pub use replay_diff::{
    basis_from_decision_data, classify_replay_diff, ReplayDiffBasis, ReplayDiffBucket,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::policy::{ApprovalArtifact, ApprovalFreshness};
    use crate::mcp::tool_definition::{
        ToolDefinitionBinding, TOOL_DEFINITION_CANONICALIZATION_JCS_MCP_TOOL_DEFINITION_V1,
        TOOL_DEFINITION_DIGEST_ALG_SHA256, TOOL_DEFINITION_SCHEMA_V1,
        TOOL_DEFINITION_SOURCE_MCP_TOOLS_LIST,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct CountingEmitter {
        count: AtomicUsize,
        last_decision: std::sync::Mutex<Option<Decision>>,
        last_reason_code: std::sync::Mutex<Option<String>>,
    }

    impl CountingEmitter {
        fn new() -> Self {
            Self {
                count: AtomicUsize::new(0),
                last_decision: std::sync::Mutex::new(None),
                last_reason_code: std::sync::Mutex::new(None),
            }
        }
    }

    impl DecisionEmitter for CountingEmitter {
        fn emit(&self, event: &DecisionEvent) {
            self.count.fetch_add(1, Ordering::SeqCst);
            *self.last_decision.lock().unwrap() = Some(event.data.decision);
            *self.last_reason_code.lock().unwrap() = Some(event.data.reason_code.clone());
        }
    }

    #[test]
    fn test_guard_explicit_allow_emits_once() {
        let emitter = Arc::new(CountingEmitter::new());
        let guard = DecisionEmitterGuard::new(
            emitter.clone(),
            "assay://test".to_string(),
            "tc_001".to_string(),
            "test_tool".to_string(),
        );

        guard.emit_allow(reason_codes::P_MANDATE_VALID);

        assert_eq!(emitter.count.load(Ordering::SeqCst), 1);
        assert_eq!(
            *emitter.last_decision.lock().unwrap(),
            Some(Decision::Allow)
        );
    }

    #[test]
    fn test_guard_explicit_deny_emits_once() {
        let emitter = Arc::new(CountingEmitter::new());
        let guard = DecisionEmitterGuard::new(
            emitter.clone(),
            "assay://test".to_string(),
            "tc_002".to_string(),
            "test_tool".to_string(),
        );

        guard.emit_deny(reason_codes::M_EXPIRED, Some("Mandate expired".to_string()));

        assert_eq!(emitter.count.load(Ordering::SeqCst), 1);
        assert_eq!(*emitter.last_decision.lock().unwrap(), Some(Decision::Deny));
        assert_eq!(
            *emitter.last_reason_code.lock().unwrap(),
            Some(reason_codes::M_EXPIRED.to_string())
        );
    }

    #[test]
    fn test_guard_drop_emits_error() {
        let emitter = Arc::new(CountingEmitter::new());
        {
            let _guard = DecisionEmitterGuard::new(
                emitter.clone(),
                "assay://test".to_string(),
                "tc_003".to_string(),
                "test_tool".to_string(),
            );
        }

        assert_eq!(emitter.count.load(Ordering::SeqCst), 1);
        assert_eq!(
            *emitter.last_decision.lock().unwrap(),
            Some(Decision::Error)
        );
        assert_eq!(
            *emitter.last_reason_code.lock().unwrap(),
            Some(reason_codes::S_INTERNAL_ERROR.to_string())
        );
    }

    #[test]
    fn test_guard_no_double_emit() {
        let emitter = Arc::new(CountingEmitter::new());
        {
            let guard = DecisionEmitterGuard::new(
                emitter.clone(),
                "assay://test".to_string(),
                "tc_004".to_string(),
                "test_tool".to_string(),
            );
            guard.emit_allow(reason_codes::P_POLICY_DENY);
        }

        assert_eq!(emitter.count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_event_serialization() {
        let event = DecisionEvent::new(
            "assay://test".to_string(),
            "tc_005".to_string(),
            "test_tool".to_string(),
        )
        .allow(reason_codes::P_MANDATE_VALID)
        .with_mandate(
            Some("sha256:abc".to_string()),
            Some("sha256:use".to_string()),
            Some(1),
        )
        .with_mandate_matches(Some(true), Some(true), Some(true));

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("assay.tool.decision"));
        assert!(json.contains("tc_005"));
        assert!(json.contains("allow"));
    }

    #[test]
    fn test_with_policy_context_sets_approval_artifact_fields() {
        let context = PolicyDecisionEventContext {
            approval_state: Some("approved".to_string()),
            approval_artifact: Some(ApprovalArtifact {
                approval_id: "apr_001".to_string(),
                approver: "alice@example.com".to_string(),
                issued_at: "2026-03-11T11:00:00Z".to_string(),
                expires_at: "2026-03-11T12:00:00Z".to_string(),
                scope: "tool:deploy".to_string(),
                bound_tool: "deploy_service".to_string(),
                bound_resource: "service/prod".to_string(),
            }),
            approval_freshness: Some(ApprovalFreshness::Fresh),
            ..PolicyDecisionEventContext::default()
        };

        let event = DecisionEvent::new(
            "assay://test".to_string(),
            "tc_006".to_string(),
            "deploy_service".to_string(),
        )
        .allow(reason_codes::P_POLICY_ALLOW)
        .with_policy_context(context);

        assert_eq!(event.data.approval_state.as_deref(), Some("approved"));
        assert_eq!(event.data.approval_id.as_deref(), Some("apr_001"));
        assert_eq!(event.data.approver.as_deref(), Some("alice@example.com"));
        assert_eq!(
            event.data.issued_at.as_deref(),
            Some("2026-03-11T11:00:00Z")
        );
        assert_eq!(
            event.data.expires_at.as_deref(),
            Some("2026-03-11T12:00:00Z")
        );
        assert_eq!(event.data.scope.as_deref(), Some("tool:deploy"));
        assert_eq!(
            event.data.approval_bound_tool.as_deref(),
            Some("deploy_service")
        );
        assert_eq!(
            event.data.approval_bound_resource.as_deref(),
            Some("service/prod")
        );
        assert_eq!(
            event.data.approval_freshness,
            Some(ApprovalFreshness::Fresh)
        );
        assert_eq!(
            event.data.decision_context_contract_version.as_deref(),
            Some(DECISION_CONTEXT_CONTRACT_VERSION_V1)
        );
        assert_eq!(
            event.data.context_payload_state,
            Some(ContextPayloadState::PartialEnvelope)
        );
        assert_eq!(
            event.data.missing_context_fields,
            vec![
                "lane".to_string(),
                "principal".to_string(),
                "auth_context_summary".to_string(),
            ]
        );
    }

    #[test]
    fn test_with_policy_context_sets_complete_context_contract_fields() {
        let context = PolicyDecisionEventContext {
            lane: Some("lane-prod".to_string()),
            principal: Some("alice@example.com".to_string()),
            auth_context_summary: Some("aud=deploy scopes=tool:deploy".to_string()),
            approval_state: Some("approved".to_string()),
            ..PolicyDecisionEventContext::default()
        };

        let event = DecisionEvent::new(
            "assay://test".to_string(),
            "tc_007".to_string(),
            "deploy_service".to_string(),
        )
        .allow(reason_codes::P_POLICY_ALLOW)
        .with_policy_context(context);

        assert_eq!(
            event.data.decision_context_contract_version.as_deref(),
            Some(DECISION_CONTEXT_CONTRACT_VERSION_V1)
        );
        assert_eq!(
            event.data.context_payload_state,
            Some(ContextPayloadState::CompleteEnvelope)
        );
        assert_eq!(
            event.data.required_context_fields,
            vec![
                "lane".to_string(),
                "principal".to_string(),
                "auth_context_summary".to_string(),
                "approval_state".to_string(),
            ]
        );
        assert!(event.data.missing_context_fields.is_empty());
    }

    #[test]
    fn test_with_policy_context_sets_delegation_fields() {
        let context = PolicyDecisionEventContext {
            delegated_from: Some("agent:planner".to_string()),
            delegation_depth: Some(1),
            ..PolicyDecisionEventContext::default()
        };

        let event = DecisionEvent::new(
            "assay://test".to_string(),
            "tc_008".to_string(),
            "deploy_service".to_string(),
        )
        .allow(reason_codes::P_POLICY_ALLOW)
        .with_policy_context(context);

        assert_eq!(event.data.delegated_from.as_deref(), Some("agent:planner"));
        assert_eq!(event.data.delegation_depth, Some(1));
    }

    #[test]
    fn test_decision_event_omits_delegation_fields_when_absent() {
        let event = DecisionEvent::new(
            "assay://test".to_string(),
            "tc_compat".to_string(),
            "deploy_service".to_string(),
        )
        .allow(reason_codes::P_POLICY_ALLOW);

        let value = serde_json::to_value(event).expect("decision event should serialize");
        let data = value
            .get("data")
            .and_then(serde_json::Value::as_object)
            .expect("decision event data should be an object");

        assert!(!data.contains_key("delegated_from"));
        assert!(!data.contains_key("delegation_depth"));
    }

    #[test]
    fn test_decision_event_omits_policy_snapshot_fields_when_digest_absent() {
        let event = DecisionEvent::new(
            "assay://test".to_string(),
            "tc_no_policy_snapshot".to_string(),
            "deploy_service".to_string(),
        )
        .allow(reason_codes::P_POLICY_ALLOW);

        let value = serde_json::to_value(event).expect("decision event should serialize");
        let data = value
            .get("data")
            .and_then(serde_json::Value::as_object)
            .expect("decision event data should be an object");

        assert!(!data.contains_key("policy_snapshot_digest"));
        assert!(!data.contains_key("policy_snapshot_digest_alg"));
        assert!(!data.contains_key("policy_snapshot_canonicalization"));
        assert!(!data.contains_key("policy_snapshot_schema"));
    }

    #[test]
    fn test_decision_event_omits_tool_definition_fields_when_binding_absent() {
        let event = DecisionEvent::new(
            "assay://test".to_string(),
            "tc_no_tool_definition".to_string(),
            "deploy_service".to_string(),
        )
        .allow(reason_codes::P_POLICY_ALLOW);

        let value = serde_json::to_value(event).expect("decision event should serialize");
        let data = value
            .get("data")
            .and_then(serde_json::Value::as_object)
            .expect("decision event data should be an object");

        assert!(!data.contains_key("tool_definition_digest"));
        assert!(!data.contains_key("tool_definition_digest_alg"));
        assert!(!data.contains_key("tool_definition_canonicalization"));
        assert!(!data.contains_key("tool_definition_schema"));
        assert!(!data.contains_key("tool_definition_source"));
    }

    #[test]
    fn test_with_policy_context_projects_policy_snapshot_digest() {
        let context = PolicyDecisionEventContext {
            policy_digest: Some("sha256:policy123".to_string()),
            ..PolicyDecisionEventContext::default()
        };

        let event = DecisionEvent::new(
            "assay://test".to_string(),
            "tc_policy_snapshot".to_string(),
            "deploy_service".to_string(),
        )
        .allow(reason_codes::P_POLICY_ALLOW)
        .with_policy_context(context);

        assert_eq!(
            event.data.policy_digest.as_deref(),
            Some("sha256:policy123")
        );
        assert_eq!(
            event.data.policy_snapshot_digest.as_deref(),
            event.data.policy_digest.as_deref()
        );
        assert_eq!(
            event.data.policy_snapshot_digest_alg.as_deref(),
            Some(POLICY_SNAPSHOT_DIGEST_ALG_SHA256)
        );
        assert_eq!(
            event.data.policy_snapshot_canonicalization.as_deref(),
            Some(POLICY_SNAPSHOT_CANONICALIZATION_JCS_MCP_POLICY)
        );
        assert_eq!(
            event.data.policy_snapshot_schema.as_deref(),
            Some(POLICY_SNAPSHOT_SCHEMA_V1)
        );
    }

    #[test]
    fn test_policy_snapshot_projection_is_atomic() {
        let event = DecisionEvent::new(
            "assay://test".to_string(),
            "tc_policy_snapshot_atomic".to_string(),
            "deploy_service".to_string(),
        )
        .allow(reason_codes::P_POLICY_ALLOW)
        .with_policy_context(PolicyDecisionEventContext {
            policy_digest: Some("sha256:policy456".to_string()),
            ..PolicyDecisionEventContext::default()
        });

        assert!(event.data.policy_snapshot_digest.is_some());
        assert!(event.data.policy_snapshot_digest_alg.is_some());
        assert!(event.data.policy_snapshot_canonicalization.is_some());
        assert!(event.data.policy_snapshot_schema.is_some());
    }

    #[test]
    fn test_with_policy_context_projects_tool_definition_binding() {
        let binding = ToolDefinitionBinding {
            digest: "sha256:tooldef123".to_string(),
            digest_alg: TOOL_DEFINITION_DIGEST_ALG_SHA256.to_string(),
            canonicalization: TOOL_DEFINITION_CANONICALIZATION_JCS_MCP_TOOL_DEFINITION_V1
                .to_string(),
            schema: TOOL_DEFINITION_SCHEMA_V1.to_string(),
            source: TOOL_DEFINITION_SOURCE_MCP_TOOLS_LIST.to_string(),
        };

        let event = DecisionEvent::new(
            "assay://test".to_string(),
            "tc_tool_definition".to_string(),
            "deploy_service".to_string(),
        )
        .allow(reason_codes::P_POLICY_ALLOW)
        .with_policy_context(PolicyDecisionEventContext {
            tool_definition_binding: Some(binding),
            ..PolicyDecisionEventContext::default()
        });

        assert_eq!(
            event.data.tool_definition_digest.as_deref(),
            Some("sha256:tooldef123")
        );
        assert_eq!(
            event.data.tool_definition_digest_alg.as_deref(),
            Some(TOOL_DEFINITION_DIGEST_ALG_SHA256)
        );
        assert_eq!(
            event.data.tool_definition_canonicalization.as_deref(),
            Some(TOOL_DEFINITION_CANONICALIZATION_JCS_MCP_TOOL_DEFINITION_V1)
        );
        assert_eq!(
            event.data.tool_definition_schema.as_deref(),
            Some(TOOL_DEFINITION_SCHEMA_V1)
        );
        assert_eq!(
            event.data.tool_definition_source.as_deref(),
            Some(TOOL_DEFINITION_SOURCE_MCP_TOOLS_LIST)
        );
    }

    #[test]
    fn test_reason_codes_are_string_constants() {
        assert_eq!(reason_codes::P_POLICY_ALLOW, "P_POLICY_ALLOW");
        assert_eq!(reason_codes::P_POLICY_DENY, "P_POLICY_DENY");
        assert_eq!(reason_codes::P_RESTRICT_SCOPE, "P_RESTRICT_SCOPE");
        assert_eq!(reason_codes::P_REDACT_ARGS, "P_REDACT_ARGS");
        assert_eq!(reason_codes::M_EXPIRED, "M_EXPIRED");
        assert_eq!(reason_codes::S_DB_ERROR, "S_DB_ERROR");
        assert_eq!(reason_codes::T_TIMEOUT, "T_TIMEOUT");
    }
}
