mod engine;
mod engine_next;
mod legacy;
mod response;
mod schema;

use super::identity::ToolIdentity;
use super::jcs;
use super::jsonrpc::JsonRpcRequest;
use super::tool_match::MatchBasis;
use super::tool_taxonomy::ToolTaxonomy;
use crate::fingerprint::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpPolicy {
    #[serde(default)]
    pub version: String,

    #[serde(default)]
    pub name: String,

    #[serde(default)]
    pub tools: ToolPolicy,

    // Legacy v1: root-level allow/deny (normalized into tools.* on load)
    #[serde(default)]
    pub allow: Option<Vec<String>>,
    #[serde(default)]
    pub deny: Option<Vec<String>>,

    /// V2: JSON Schema per tool (primary)
    #[serde(default)]
    pub schemas: HashMap<String, Value>,

    /// V1 (deprecated): Regex constraints - auto-converted to schemas on load
    #[serde(default, deserialize_with = "deserialize_constraints")]
    pub constraints: Vec<ConstraintRule>,

    #[serde(default)]
    pub enforcement: EnforcementSettings,

    #[serde(default)]
    pub limits: Option<GlobalLimits>,

    #[serde(default)]
    pub signatures: Option<SignaturePolicy>,

    /// Cryptographic pins for tool integrity (Phase 9)
    #[serde(default)]
    pub tool_pins: HashMap<String, ToolIdentity>,

    #[serde(default, flatten)]
    pub tool_taxonomy: ToolTaxonomy,

    // Phase 4: Runtime Features
    #[serde(default)]
    pub discovery: Option<DiscoveryConfig>,
    #[serde(default)]
    pub runtime_monitor: Option<RuntimeMonitorConfig>,
    #[serde(default)]
    pub kill_switch: Option<KillSwitchConfig>,

    /// Compiled schemas (lazy, thread-safe, shared across clones)
    #[serde(skip)]
    pub(crate) compiled: Arc<OnceLock<HashMap<String, Arc<jsonschema::Validator>>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforcementSettings {
    /// What to do when a tool has no schema
    #[serde(default = "default_unconstrained")]
    pub unconstrained_tools: UnconstrainedMode,
}

impl Default for EnforcementSettings {
    fn default() -> Self {
        Self {
            unconstrained_tools: UnconstrainedMode::Warn,
        }
    }
}

fn default_unconstrained() -> UnconstrainedMode {
    UnconstrainedMode::Warn
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UnconstrainedMode {
    #[default]
    Warn,
    Deny,
    Allow,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SignaturePolicy {
    #[serde(default)]
    pub check_descriptions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalLimits {
    pub max_requests_total: Option<u64>,
    pub max_tool_calls_total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolPolicy {
    pub allow: Option<Vec<String>>,
    pub deny: Option<Vec<String>>,
    #[serde(default)]
    pub allow_classes: Option<Vec<String>>,
    #[serde(default)]
    pub deny_classes: Option<Vec<String>>,
    #[serde(default)]
    pub approval_required: Option<Vec<String>>,
    #[serde(default)]
    pub approval_required_classes: Option<Vec<String>>,
    #[serde(default)]
    pub restrict_scope: Option<Vec<String>>,
    #[serde(default)]
    pub restrict_scope_classes: Option<Vec<String>>,
    #[serde(default)]
    pub restrict_scope_contract: Option<RestrictScopeContract>,
    #[serde(default)]
    pub redact_args: Option<Vec<String>>,
    #[serde(default)]
    pub redact_args_classes: Option<Vec<String>>,
    #[serde(default)]
    pub redact_args_contract: Option<RedactArgsContract>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PolicyMatchMetadata {
    pub tool_classes: Vec<String>,
    pub matched_tool_classes: Vec<String>,
    pub match_basis: MatchBasis,
    pub matched_rule: Option<String>,
    pub typed_decision: Option<TypedPolicyDecision>,
    pub policy_version: Option<String>,
    pub policy_digest: Option<String>,
    pub obligations: Vec<PolicyObligation>,
    pub approval_state: Option<String>,
    pub approval_artifact: Option<ApprovalArtifact>,
    pub approval_freshness: Option<ApprovalFreshness>,
    pub approval_failure_reason: Option<String>,
    pub scope_type: Option<String>,
    pub scope_value: Option<String>,
    pub scope_match_mode: Option<String>,
    pub scope_evaluation_state: Option<String>,
    pub scope_failure_reason: Option<String>,
    pub restrict_scope_present: Option<bool>,
    pub restrict_scope_target: Option<String>,
    pub restrict_scope_match: Option<bool>,
    pub restrict_scope_reason: Option<String>,
    pub redaction_target: Option<String>,
    pub redaction_mode: Option<String>,
    pub redaction_scope: Option<String>,
    pub redaction_applied_state: Option<String>,
    pub redaction_reason: Option<String>,
    pub redaction_failure_reason: Option<String>,
    pub redact_args_present: Option<bool>,
    pub redact_args_target: Option<String>,
    pub redact_args_mode: Option<String>,
    pub redact_args_result: Option<String>,
    pub redact_args_reason: Option<String>,
    pub fail_closed: Option<FailClosedContext>,
    pub lane: Option<String>,
    pub principal: Option<String>,
    pub auth_context_summary: Option<String>,
    /// G3 v1: `oauth2` | `jwt_bearer` when policy-projected
    pub auth_scheme: Option<String>,
    /// G3 v1: trimmed issuer (`iss`) string
    pub auth_issuer: Option<String>,
    pub delegated_from: Option<String>,
    pub delegation_depth: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolicyEvaluation {
    pub decision: PolicyDecision,
    pub metadata: PolicyMatchMetadata,
}

// Canonical Rule Shape (Legacy V1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintRule {
    pub tool: String,
    pub params: BTreeMap<String, ConstraintParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintParam {
    #[serde(default)]
    pub matches: Option<String>,
}

pub use super::runtime_features::{
    ActionLevel, DiscoveryActions, DiscoveryConfig, DiscoveryMethod, KillMode, KillSwitchConfig,
    KillTrigger, MonitorAction, MonitorMatch, MonitorProvider, MonitorRule, MonitorRuleType,
    RuntimeMonitorConfig,
};

#[derive(Debug, Default)]
pub struct PolicyState {
    pub requests_count: u64,
    pub tool_calls_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PolicyDecision {
    Allow,
    AllowWithWarning {
        tool: String,
        code: String,
        reason: String,
    },
    Deny {
        tool: String,
        code: String,
        reason: String,
        contract: Value,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TypedPolicyDecision {
    Allow,
    #[serde(rename = "allow_with_obligations")]
    AllowWithObligations,
    Deny,
    #[serde(rename = "deny_with_alert")]
    DenyWithAlert,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyObligation {
    #[serde(rename = "type")]
    pub obligation_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restrict_scope: Option<RestrictScopeContract>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redact_args: Option<RedactArgsContract>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalArtifact {
    pub approval_id: String,
    pub approver: String,
    pub issued_at: String,
    pub expires_at: String,
    pub scope: String,
    pub bound_tool: String,
    pub bound_resource: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestrictScopeContract {
    pub scope_type: String,
    pub scope_value: String,
    pub scope_match_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedactArgsContract {
    pub redaction_target: String,
    pub redaction_mode: String,
    pub redaction_scope: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalFreshness {
    Fresh,
    Stale,
    Expired,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolRiskClass {
    HighRisk,
    LowRiskRead,
    Default,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailClosedMode {
    FailClosed,
    DegradeReadOnly,
    FailSafeAllow,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailClosedTrigger {
    PolicyEngineUnavailable,
    ContextProviderUnavailable,
    RuntimeDependencyError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailClosedContext {
    pub tool_risk_class: ToolRiskClass,
    pub fail_closed_mode: FailClosedMode,
    pub fail_closed_trigger: Option<FailClosedTrigger>,
    pub fail_closed_applied: bool,
    pub fail_closed_error_code: Option<String>,
}

impl PolicyObligation {
    pub fn warning_compat(code: &str, reason: &str) -> Self {
        Self {
            obligation_type: "legacy_warning".to_string(),
            detail: Some(format!("{code}:{reason}")),
            restrict_scope: None,
            redact_args: None,
        }
    }

    pub fn alert(code: &str, reason: &str) -> Self {
        Self {
            obligation_type: "alert".to_string(),
            detail: Some(format!("{code}:{reason}")),
            restrict_scope: None,
            redact_args: None,
        }
    }

    pub fn restrict_scope(contract: RestrictScopeContract, detail: Option<String>) -> Self {
        Self {
            obligation_type: "restrict_scope".to_string(),
            detail,
            restrict_scope: Some(contract),
            redact_args: None,
        }
    }

    pub fn redact_args(contract: RedactArgsContract, detail: Option<String>) -> Self {
        Self {
            obligation_type: "redact_args".to_string(),
            detail,
            restrict_scope: None,
            redact_args: Some(contract),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyDecisionContract {
    pub decision: TypedPolicyDecision,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub obligations: Vec<PolicyObligation>,
}

impl PolicyDecision {
    pub fn typed_contract(&self) -> PolicyDecisionContract {
        match self {
            Self::Allow => PolicyDecisionContract {
                decision: TypedPolicyDecision::Allow,
                obligations: Vec::new(),
            },
            Self::AllowWithWarning { code, reason, .. } => PolicyDecisionContract {
                decision: TypedPolicyDecision::AllowWithObligations,
                obligations: vec![PolicyObligation::warning_compat(code, reason)],
            },
            Self::Deny { code, reason, .. } if is_alert_deny_code(code) => PolicyDecisionContract {
                decision: TypedPolicyDecision::DenyWithAlert,
                obligations: vec![PolicyObligation::alert(code, reason)],
            },
            Self::Deny { .. } => PolicyDecisionContract {
                decision: TypedPolicyDecision::Deny,
                obligations: Vec::new(),
            },
        }
    }
}

fn is_alert_deny_code(code: &str) -> bool {
    matches!(code, "E_TOOL_DRIFT")
}

// Dual-Shape Deserializer Helper (Legacy)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ConstraintsCompat {
    List(Vec<ConstraintRule>),
    Map(BTreeMap<String, BTreeMap<String, InputParamConstraint>>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum InputParamConstraint {
    Direct(String),
    Object(ConstraintParam),
}

fn deserialize_constraints<'de, D>(d: D) -> Result<Vec<ConstraintRule>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let c = Option::<ConstraintsCompat>::deserialize(d)?;
    let out = match c {
        None => vec![],
        Some(ConstraintsCompat::List(v)) => v,
        Some(ConstraintsCompat::Map(m)) => m
            .into_iter()
            .map(|(tool, params)| {
                let new_params = params
                    .into_iter()
                    .map(|(arg, val)| {
                        let param = match val {
                            InputParamConstraint::Direct(s) => ConstraintParam { matches: Some(s) },
                            InputParamConstraint::Object(o) => o,
                        };
                        (arg, param)
                    })
                    .collect();
                ConstraintRule {
                    tool,
                    params: new_params,
                }
            })
            .collect(),
    };
    Ok(out)
}

fn matches_tool_pattern(tool_name: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return tool_name == pattern;
    }
    let starts_star = pattern.starts_with('*');
    let ends_star = pattern.ends_with('*');
    match (starts_star, ends_star) {
        (true, true) => {
            let inner = pattern.trim_matches('*');
            if inner.is_empty() {
                true
            } else {
                tool_name.contains(inner)
            }
        }
        (false, true) => {
            let prefix = pattern.trim_end_matches('*');
            !prefix.is_empty() && tool_name.starts_with(prefix)
        }
        (true, false) => {
            let suffix = pattern.trim_start_matches('*');
            !suffix.is_empty() && tool_name.ends_with(suffix)
        }
        (false, false) => tool_name == pattern,
    }
}

impl McpPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        legacy::from_file(path)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        legacy::validate(self)
    }

    pub fn is_v1_format(&self) -> bool {
        legacy::is_v1_format(self)
    }

    /// Normalize legacy root-level allow/deny into tools.allow/deny.
    pub fn normalize_legacy_shapes(&mut self) {
        legacy::normalize_legacy_shapes(self);
    }

    /// Migrate V1 regex constraints to V2 JSON Schemas.
    /// Warning: This clears the `constraints` field.
    pub fn migrate_constraints_to_schemas(&mut self) {
        schema::migrate_constraints_to_schemas(self);
    }

    fn compiled_schemas(&self) -> &HashMap<String, Arc<jsonschema::Validator>> {
        self.compiled
            .get_or_init(|| schema::compile_all_schemas(self))
    }

    pub fn compile_all_schemas(&self) -> HashMap<String, Arc<jsonschema::Validator>> {
        schema::compile_all_schemas(self)
    }

    pub fn policy_digest(&self) -> Option<String> {
        let canonical = jcs::to_string(self).ok()?;
        Some(format!("sha256:{}", sha256_hex(&canonical)))
    }

    /// Single evaluation entry point for CLI and Server
    pub fn evaluate(
        &self,
        tool_name: &str,
        args: &Value,
        state: &mut PolicyState,
        runtime_identity: Option<&ToolIdentity>,
    ) -> PolicyDecision {
        self.evaluate_with_metadata(tool_name, args, state, runtime_identity)
            .decision
    }

    pub fn evaluate_with_metadata(
        &self,
        tool_name: &str,
        args: &Value,
        state: &mut PolicyState,
        runtime_identity: Option<&ToolIdentity>,
    ) -> PolicyEvaluation {
        engine::evaluate_with_metadata(self, tool_name, args, state, runtime_identity)
    }

    // Proxy-specific check method (Legacy compatibility wrapper)
    pub fn check(&self, request: &JsonRpcRequest, state: &mut PolicyState) -> PolicyDecision {
        engine::check(self, request, state)
    }
}

pub use response::make_deny_response;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn typed_contract_maps_allow_with_warning_to_legacy_warning_obligation() {
        let decision = PolicyDecision::AllowWithWarning {
            tool: "tool_a".to_string(),
            code: "E_TOOL_UNCONSTRAINED".to_string(),
            reason: "Tool allowed but has no schema".to_string(),
        };

        let contract = decision.typed_contract();
        assert_eq!(contract.decision, TypedPolicyDecision::AllowWithObligations);
        assert_eq!(contract.obligations.len(), 1);
        assert_eq!(contract.obligations[0].obligation_type, "legacy_warning");
    }

    #[test]
    fn typed_contract_maps_tool_drift_to_deny_with_alert_obligation() {
        let decision = PolicyDecision::Deny {
            tool: "tool_a".to_string(),
            code: "E_TOOL_DRIFT".to_string(),
            reason: "Tool drifted".to_string(),
            contract: json!({ "status": "deny" }),
        };

        let contract = decision.typed_contract();
        assert_eq!(contract.decision, TypedPolicyDecision::DenyWithAlert);
        assert_eq!(contract.obligations.len(), 1);
        assert_eq!(contract.obligations[0].obligation_type, "alert");
        assert_eq!(
            contract.obligations[0].detail.as_deref(),
            Some("E_TOOL_DRIFT:Tool drifted")
        );
    }

    #[test]
    fn typed_contract_maps_regular_deny_without_obligations() {
        let decision = PolicyDecision::Deny {
            tool: "tool_a".to_string(),
            code: "E_TOOL_DENIED".to_string(),
            reason: "Denied".to_string(),
            contract: json!({ "status": "deny" }),
        };

        let contract = decision.typed_contract();
        assert_eq!(contract.decision, TypedPolicyDecision::Deny);
        assert!(contract.obligations.is_empty());
    }

    #[test]
    fn restrict_scope_obligation_preserves_typed_shape() {
        let obligation = PolicyObligation::restrict_scope(
            RestrictScopeContract {
                scope_type: "resource".to_string(),
                scope_value: "service/prod".to_string(),
                scope_match_mode: "exact".to_string(),
            },
            Some("shape-only contract".to_string()),
        );

        assert_eq!(obligation.obligation_type, "restrict_scope");
        assert_eq!(
            obligation.restrict_scope,
            Some(RestrictScopeContract {
                scope_type: "resource".to_string(),
                scope_value: "service/prod".to_string(),
                scope_match_mode: "exact".to_string(),
            })
        );
    }

    #[test]
    fn redact_args_obligation_preserves_typed_shape() {
        let obligation = PolicyObligation::redact_args(
            RedactArgsContract {
                redaction_target: "body".to_string(),
                redaction_mode: "mask".to_string(),
                redaction_scope: "request".to_string(),
            },
            Some("shape-only contract".to_string()),
        );

        assert_eq!(obligation.obligation_type, "redact_args");
        assert_eq!(
            obligation.redact_args,
            Some(RedactArgsContract {
                redaction_target: "body".to_string(),
                redaction_mode: "mask".to_string(),
                redaction_scope: "request".to_string(),
            })
        );
    }
}
