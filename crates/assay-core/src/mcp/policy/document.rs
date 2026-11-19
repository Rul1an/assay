//! The policy file, before defaults and legacy constraints are normalized for enforcement.
use super::deserialize::{normalize_constraints, LegacyConstraints};
use super::types::*;
use super::{McpPolicyError, McpPolicyErrorKind};
use crate::mcp::{identity::ToolIdentity, tool_taxonomy::ToolTaxonomy};
use assay_common::closed;
use assay_common::doc::{load, Opaque, Refusal};
use std::collections::{BTreeSet, HashMap};

closed! {
    root(yaml_types, x_extensions) struct PolicyDocument {
        #[default]
        version: String,
        #[default]
        name: String,
        #[default]
        tools: DocumentTools,
        allow: Option<Vec<String>>,
        deny: Option<Vec<String>>,
        #[default]
        schemas: HashMap<String, Opaque<serde_json::Value>>,
        constraints: Option<LegacyConstraints>,
        #[default]
        enforcement: EnforcementSettings,
        limits: Option<GlobalLimits>,
        signatures: Option<SignaturePolicy>,
        #[default]
        tool_pins: HashMap<String, ToolIdentity>,
        #[default]
        tool_classes: HashMap<String, BTreeSet<String>>,
        /// Read by assay_check_sequence.
        sequences: Option<Vec<crate::model::SequenceRule>>,
        /// Read by assay_policy_decide.
        blocklist: Option<Vec<String>>,
        /// Read by assay_check_sequence.
        aliases: Option<HashMap<String, Vec<String>>>,
        discovery: Option<DiscoveryConfig>,
        runtime_monitor: Option<RuntimeMonitorConfig>,
        kill_switch: Option<KillSwitchConfig>,
    }
}

pub(super) fn read(text: &str) -> Result<McpPolicy, McpPolicyError> {
    let PolicyDocument {
        version,
        name,
        tools,
        allow,
        deny,
        schemas,
        constraints,
        enforcement,
        limits,
        signatures,
        tool_pins,
        tool_classes,
        sequences: _,
        blocklist: _,
        aliases: _,
        discovery,
        runtime_monitor,
        kill_switch,
    } = load(text).map_err(refusal)?;
    Ok(McpPolicy {
        version,
        name,
        tools: tools.into(),
        allow,
        deny,
        schemas: schemas
            .into_iter()
            .map(|(tool, schema)| (tool, schema.0))
            .collect(),
        constraints: normalize_constraints(constraints),
        enforcement,
        limits,
        signatures,
        tool_pins,
        tool_taxonomy: ToolTaxonomy { tool_classes },
        discovery,
        runtime_monitor,
        kill_switch,
        compiled: Default::default(),
    })
}

fn refusal(error: Refusal) -> McpPolicyError {
    use McpPolicyErrorKind::{RootNotMapping, Structure, Syntax};
    let (kind, source) = match error {
        Refusal::Syntax {
            line,
            column,
            message,
        } => (Syntax { line, column }, anyhow::anyhow!("{message}")),
        Refusal::UnknownKeys(keys) => (
            McpPolicyErrorKind::UnknownKeys,
            anyhow::anyhow!("unknown policy keys: {keys:?}"),
        ),
        Refusal::NotAMapping { path, .. } if path.is_empty() => (
            RootNotMapping,
            anyhow::anyhow!("policy root is not a YAML mapping"),
        ),
        Refusal::NotAMapping { path, .. } => (
            Structure,
            anyhow::anyhow!("expected a mapping at policy key {path:?}"),
        ),
        Refusal::Shape { message, .. } => (Structure, anyhow::anyhow!("{message}")),
        _ => (Structure, anyhow::anyhow!("invalid policy document")),
    };
    McpPolicyError { kind, source }
}
