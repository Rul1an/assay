use super::super::identity::ToolIdentity;
use super::super::tool_match::MatchBasis;
use super::super::tool_taxonomy::ToolTaxonomy;
use assay_common::closed;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};

/// Per-tool compile results, materialized once per policy. `Err` carries the preparation or
/// compile failure for that tool; the enforcement engine denies such a tool's calls with
/// `E_SCHEMA_COMPILE` instead of aborting (#1952).
pub(crate) type CompiledSchemas = HashMap<String, Result<Arc<jsonschema::Validator>, String>>;

#[derive(Debug, Clone, Serialize, Default)]
pub struct McpPolicy {
    #[serde(default)]
    pub version: String,

    #[serde(default)]
    pub name: String,

    #[serde(default)]
    pub tools: ToolPolicy,

    /// V2: JSON Schema per tool (primary)
    #[serde(default)]
    pub schemas: HashMap<String, Value>,

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
    pub(crate) compiled: Arc<OnceLock<CompiledSchemas>>,
}

closed! {
    #[derive(Serialize)]
    pub struct EnforcementSettings {
        /// What to do when a tool has no schema
        #[default = "default_unconstrained"]
        unconstrained_tools: UnconstrainedMode,
    }
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

closed! {
    #[derive(Serialize, Default)]
    pub enum UnconstrainedMode {
        #[default]
        "warn" => Warn,
        "deny" => Deny,
        "allow" => Allow,
    }
}

closed! {
    #[derive(Serialize, Default)]
    pub struct SignaturePolicy {
        #[default]
        check_descriptions: bool,
    }
}

closed! {
    #[derive(Serialize, Default)]
    pub struct GlobalLimits {
        max_requests_total: Option<u64>,
        max_tool_calls_total: Option<u64>,
    }
}

// The public enforcement configuration has no coverage field. Both representations
// take the same enforcement fields, so adding one also updates the projection.
macro_rules! tool_policy {
    ($first:ident : $first_ty:ty, $second:ident : $second_ty:ty, $( $(#[$attr:meta])* $field:ident : $ty:ty ),+ $(,)?) => {
        #[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
        pub struct ToolPolicy {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub $first: Option<$first_ty>,
            pub $second: Option<$second_ty>,
            $( $(#[$attr])* #[serde(default)] pub $field: Option<$ty>, )+
        }

        closed! {
            #[derive(Default)]
            pub(super) struct DocumentTools {
                $first: Option<$first_ty>,
                $second: Option<$second_ty>,
                $( $field: Option<$ty>, )+
                /// Read by assay coverage.
                require_args: Option<HashMap<String, Vec<String>>>,
            }
        }

        closed! {
            #[derive(Default, Serialize)]
            pub(super) struct CoverageTools {
                $first: Option<$first_ty>,
                $second: Option<$second_ty>,
                $( $field: Option<$ty>, )+
                require_args: Option<HashMap<String, Vec<String>>>,
                arg_constraints: Option<assay_common::doc::Opaque<HashMap<String, HashMap<String, Value>>>>,
            }
        }

        impl CoverageTools {
            pub(super) fn split(self) -> (DocumentTools, crate::model::ToolsPolicy) {
                let Self { $first, $second, $( $field, )+ require_args, arg_constraints } = self;
                let coverage = crate::model::ToolsPolicy {
                    allow: None,
                    deny: None,
                    require_args,
                    arg_constraints: arg_constraints.map(|constraints| constraints.0),
                };
                (DocumentTools { $first, $second, $( $field, )+ require_args: None }, coverage)
            }
        }

        impl From<DocumentTools> for ToolPolicy {
            fn from(tools: DocumentTools) -> Self {
                let DocumentTools { $first, $second, $( $field, )+ require_args: _ } = tools;
                Self { $first, $second, $( $field, )+ }
            }
        }
    };
}

tool_policy! {
    allow: Vec<String>,
    deny: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allow_classes: Vec<String>,
    deny_classes: Vec<String>,
    approval_required: Vec<String>,
    approval_required_classes: Vec<String>,
    restrict_scope: Vec<String>,
    restrict_scope_classes: Vec<String>,
    restrict_scope_contract: RestrictScopeContract,
    redact_args: Vec<String>,
    redact_args_classes: Vec<String>,
    redact_args_contract: RedactArgsContract,
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
closed! {
    #[derive(Serialize)]
    legacy_positional pub(super) struct ConstraintRule {
        tool: String,
        params: BTreeMap<String, ConstraintParam>,
    }
}

closed! {
    #[derive(Serialize)]
    legacy_positional pub(super) struct ConstraintParam {
        #[default]
        matches: Option<String>,
    }
}

pub use super::super::runtime_features::{
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

closed! {
    #[derive(Serialize, Eq)]
    pub struct RestrictScopeContract {
        scope_type: String,
        scope_value: String,
        scope_match_mode: String,
    }
}

closed! {
    #[derive(Serialize, Eq)]
    pub struct RedactArgsContract {
        redaction_target: String,
        redaction_mode: String,
        redaction_scope: String,
    }
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
